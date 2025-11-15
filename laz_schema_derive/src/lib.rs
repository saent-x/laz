extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::{
    Data, DeriveInput, Fields, GenericArgument, PathArguments, Type, TypePath, parse_macro_input,
};

#[proc_macro_derive(LazSchema)]
pub fn derive_laz_schema(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let type_name = input.ident.to_string();

    let schema = match &input.data {
        Data::Struct(data) => generate_struct_schema(&type_name, &data.fields),
        Data::Enum(data) => generate_enum_schema(&type_name, &data.variants),
        Data::Union(_) => panic!("Unions not supported for LazSchema derive"),
    };

    let schema_fn = syn::Ident::new(
        &format!("__laz_build_schema_{}", type_name),
        proc_macro2::Span::call_site(),
    );
    let getter_fn = syn::Ident::new(
        &format!("__laz_get_schema_{}", type_name),
        proc_macro2::Span::call_site(),
    );
    let type_name_literal = proc_macro2::Literal::string(&type_name);

    let expanded = quote! {
        fn #schema_fn() -> laz_types::TypeSchema {
            #schema
        }

        fn #getter_fn() -> &'static laz_types::TypeSchema {
            static SCHEMA: ::std::sync::OnceLock<laz_types::TypeSchema> = ::std::sync::OnceLock::new();
            SCHEMA.get_or_init(|| #schema_fn())
        }

        #[allow(non_upper_case_globals)]
        const _: fn() = || {
            ::inventory::submit! {
                laz_types::TypeSchemaEntry {
                    type_name: #type_name_literal,
                    getter: #getter_fn,
                }
            };
        };
    };

    TokenStream::from(expanded)
}

fn generate_enum_schema(
    type_name: &str,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::token::Comma>,
) -> proc_macro2::TokenStream {
    let variant_schemas = variants.iter().map(|v| {
        let variant_name = v.ident.to_string();
        let inner_schema = match &v.fields {
            Fields::Unit => quote! { None },
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 => {
                let field_ty = &fields.unnamed[0].ty;
                let inner = type_to_schema(field_ty);
                quote! { Some(Box::new(#inner)) }
            }
            _ => quote! { None }, // Complex variants treated as opaque
        };

        quote! {
            laz_types::VariantSchema {
                variant_name: #variant_name.to_string(),
                inner_schema: #inner_schema,
            }
        }
    });

    quote! {
        laz_types::TypeSchema::Enum(laz_types::EnumSchema {
            type_name: #type_name.to_string(),
            variants: vec![#(#variant_schemas),*],
        })
    }
}

/// Generates schema for struct fields
fn generate_struct_schema(type_name: &str, fields: &Fields) -> proc_macro2::TokenStream {
    match fields {
        Fields::Named(fields) => {
            let field_schemas = fields.named.iter().map(|f| {
                let field_name = f.ident.as_ref().unwrap().to_string();
                let is_optional = is_optional_type(&f.ty);
                let field_type = type_to_schema(&f.ty);

                quote! {
                    laz_types::FieldSchema {
                        field_name: #field_name.to_string(),
                        field_type: Box::new(#field_type),
                        optional: #is_optional,
                    }
                }
            });

            quote! {
                laz_types::TypeSchema::Struct(laz_types::StructSchema {
                    type_name: #type_name.to_string(),
                    fields: vec![#(#field_schemas),*],
                })
            }
        }

        Fields::Unnamed(_) => {
            // Tuple struct - treat as opaque
            quote! {
                laz_types::TypeSchema::Opaque(#type_name.to_string())
            }
        }

        Fields::Unit => {
            // Unit struct
            quote! {
                laz_types::TypeSchema::Struct(laz_types::StructSchema {
                    type_name: #type_name.to_string(),
                    fields: vec![],
                })
            }
        }
    }
}

fn type_to_schema(ty: &Type) -> proc_macro2::TokenStream {
    match ty {
        Type::Path(type_path) => {
            // Check if it's a container type (Vec, Option, Result)
            if let Some(container) = get_container_type(type_path) {
                let inner_schema = type_to_schema(get_inner_type(type_path).unwrap());
                
                quote! {
                    laz_types::TypeSchema::Container {
                        container_type: #container.to_string(),
                        inner_type: Box::new(#inner_schema)
                    }
                }
            } else {
                let type_str = quote::quote!(#type_path).to_string();
                // Primitive or custom type
                quote! {
                    laz_types::TypeSchema::Primitive(#type_str.to_string())
                }
            }
        }

        Type::Reference(_) => {
            let type_str = quote!(ty).to_string();
            quote! {
                laz_types::TypeSchema::Primitive(#type_str.to_string())
            }
        }

        _ => {
            let type_str = quote::quote!(#ty).to_string();
            quote! {
                laz_types::TypeSchema::Opaque(#type_str.to_string())
            }
        }
    }
}

/// Check if type is Optional<T>
fn is_optional_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

/// Extract inner type from Option<T> or Vec<T>
fn get_inner_type(type_path: &TypePath) -> Option<&Type> {
    if let Some(segment) = type_path.path.segments.last() {
        if let PathArguments::AngleBracketed(args) = &segment.arguments {
            if let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                return Some(inner_ty);
            }
        }
    }
    None
}

/// Check if type is a container and return container name
fn get_container_type(type_path: &TypePath) -> Option<&'static str> {
    type_path
        .path
        .segments
        .last()
        .and_then(|segments| match segments.ident.to_string().as_str() {
            "Vec" => Some("Vec"),
            "Option" => Some("Option"),
            "Result" => Some("Result"),
            _ => None,
        })
}
