extern crate proc_macro;
use proc_macro::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, Pat, ReturnType, Type, TypePath, parse_macro_input};

/// Helper struct to hold parameter information during macro expansion
struct ParamInfoParts {
    name: String,
    full_type: String,
    extractor: String,
    inner_type: String,
    inner_type_path: Option<TypePath>,
}

/// Marks a function as an RPC query (GET request handler)
/// Place this OUTERMOST (above #[debug_handler] and route macros)
#[proc_macro_attribute]
pub fn rpc_query(attr: TokenStream, item: TokenStream) -> TokenStream {
    build_metadata(attr, item, false)
}

/// Marks a function as an RPC mutation (POST/PUT/PATCH/DELETE handler)
/// Place this OUTERMOST (above #[debug_handler] and route macros)
#[proc_macro_attribute]
pub fn rpc_mutation(attr: TokenStream, item: TokenStream) -> TokenStream {
    build_metadata(attr, item, true)
}

/// Shared implementation that extracts metadata and registers it
fn build_metadata(attr: TokenStream, item: TokenStream, is_mutation: bool) -> TokenStream {
    let input_fn = parse_macro_input!(item as ItemFn); // parse into syntax tree representing the function
    let function_name_ident = &input_fn.sig.ident; // Extract the function name identifier
    let function_name_str = function_name_ident.to_string(); // Convert to string for storage in metadata
    let is_async = input_fn.sig.asyncness.is_some(); // Check if function is async
    let params_parts = extract_params(&input_fn.sig.inputs);
    let return_type_str = extract_return_type(&input_fn.sig.output);
    let attrs = &input_fn.attrs;
    let vis = &input_fn.vis; // Preserve visibility (pub, pub(crate), etc.)
    let sig = &input_fn.sig; // Preserve function signature (name, generics, parameters, return type)
    let block = &input_fn.block; // Preserve function body/block
    let params_array = build_params_array(&params_parts);

    // Parse attribute arguments: input=Type, output=Type
    let (attr_input, attr_output) = parse_io_attr(attr);
    // Infer input type name if not provided: take first param with an inner_type_path
    let inferred_input = params_parts.iter().find_map(|p| {
        p.inner_type_path
            .as_ref()
            .map(|tp| tp.path.segments.last().unwrap().ident.to_string())
    });
    let input_type_name = attr_input.or(inferred_input);

    // Output is required; if not provided, emit a compile error
    let output_type_name = match attr_output {
        Some(t) => t,
        None => {
            return syn::Error::new_spanned(
                &input_fn.sig.ident,
                "rpc_query/rpc_mutation requires an `output = TypeName` attribute",
            )
            .to_compile_error()
            .into();
        }
    };

    // Prepare tokens as string literals for interpolation
    let input_type_name_tokens: proc_macro2::TokenStream = if let Some(s) = &input_type_name {
        let lit = proc_macro2::Literal::string(s);
        quote::quote! { Some(#lit.to_string()) }
    } else {
        quote::quote! { None }
    };
    let output_type_name_lit = proc_macro2::Literal::string(&output_type_name);

    let metadata_fn = syn::Ident::new(
        &format!("__laz_get_metadata_{}", function_name_str),
        proc_macro2::Span::call_site(),
    );
    let function_name_lit = proc_macro2::Literal::string(&function_name_str);

    // Generate the final output code
    let expanded = quote! {
        // PRESERVE ORIGINAL FUNCTION
        #(#attrs)*
        #vis #sig #block
        
        fn #metadata_fn() -> &'static laz_types::FunctionMetadata {
            static METADATA: ::std::sync::OnceLock<laz_types::FunctionMetadata> = ::std::sync::OnceLock::new();
            METADATA.get_or_init(|| {
                laz_types::FunctionMetadata {
                    function_name: #function_name_str.to_owned(),
                    params: #params_array,
                    return_type: laz_types::TypeSchema::Primitive(#return_type_str.to_owned()),
                    input_type_name: #input_type_name_tokens,
                    output_type_name: #output_type_name_lit.to_owned(),
                    is_async: #is_async,
                    is_mutation: #is_mutation,
                }
            })
        }

        #[allow(non_upper_case_globals)]
        const _: fn() = || {
            ::inventory::submit! {
                laz_types::FunctionMetadataEntry {
                    function_name: #function_name_lit,
                    getter: #metadata_fn,
                }
            };
        };
    };

    // Convert back to TokenStream for the compiler
    TokenStream::from(expanded)
}



/// Parse attribute like: #[rpc_query(input = Foo, output = Bar)]
fn parse_io_attr(attr: TokenStream) -> (Option<String>, Option<String>) {
    let ts = proc_macro2::TokenStream::from(attr);
    let mut input_ty: Option<String> = None;
    let mut output_ty: Option<String> = None;

    // Very small hand-rolled parser: key = Type, separated by commas
    let mut iter = ts.into_iter().peekable();
    while let Some(tt) = iter.next() {
        if let proc_macro2::TokenTree::Ident(ident) = tt {
            let key = ident.to_string();
            // expect '='
            if let Some(proc_macro2::TokenTree::Punct(p)) = iter.next() {
                if p.as_char() != '=' {
                    continue;
                }
            } else {
                continue;
            }
            // parse a Type path (sequence of Idens and '::' and generics - we only capture last ident as name)
            let mut ty_str = String::new();
            let mut depth: i32 = 0;
            while let Some(next) = iter.peek() {
                match next {
                    proc_macro2::TokenTree::Punct(p) if depth == 0 && p.as_char() == ',' => break,
                    proc_macro2::TokenTree::Group(g) => {
                        ty_str.push_str(&g.stream().to_string());
                        depth += 1;
                        iter.next();
                    }
                    other => {
                        ty_str.push_str(&other.to_string());
                        iter.next();
                    }
                }
            }
            // consume optional trailing comma
            if let Some(proc_macro2::TokenTree::Punct(p)) = iter.peek() {
                if p.as_char() == ',' {
                    iter.next();
                }
            }

            // Reduce type path string to last segment as a conservative type "name"
            let type_name = ty_str
                .split("::")
                .last()
                .map(|s| s.trim().trim_matches('<').trim_matches('>'))
                .unwrap_or(&ty_str)
                .to_string();

            if key == "input" {
                input_ty = Some(type_name);
            } else if key == "output" {
                output_ty = Some(type_name);
            }
        }
    }

    (input_ty, output_ty)
}

fn extract_return_type(output: &ReturnType) -> String {
    match output {
        ReturnType::Default => "()".to_string(),
        ReturnType::Type(_, ty) => quote::quote!(#ty).to_string(),
    }
}

/// Extracts detailed information from function parameters
fn extract_params(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::token::Comma>,
) -> Vec<ParamInfoParts> {
    let mut params = Vec::new();

    for input in inputs {
        if let FnArg::Typed(pat_type) = input {
            // Extract the binding name (e.g., "params" in `Json(params)`)
            if let Pat::Ident(pat_ident) = &*pat_type.pat {
                let name = pat_ident.ident.to_string();

                // Parse the type annotation (e.g., `Json<RegisterParams>`)
                if let Type::Path(type_path) = &*pat_type.ty {
                    let (extractor, inner_type, inner_type_path) =
                        parse_extractor_with_path(type_path);

                    params.push(ParamInfoParts {
                        name,
                        full_type: quote::quote!(#type_path).to_string(),
                        extractor,
                        inner_type,
                        inner_type_path, // For schema lookup
                    });
                } else {
                    // Fallback for non-path types (e.g., references, slices)
                    let full_type = quote::quote!(#pat_type.ty).to_string();
                    params.push(ParamInfoParts {
                        name,
                        full_type,
                        extractor: "Unknown".to_string(),
                        inner_type: "Unknown".to_string(),
                        inner_type_path: None,
                    });
                }
            }
        }
    }

    params
}

/// Parses extractor type and captures the TypePath for schema lookup
fn parse_extractor_with_path(type_path: &TypePath) -> (String, String, Option<TypePath>) {
    if let Some(segment) = type_path.path.segments.last() {
        let extractor = segment.ident.to_string();

        // Check for generic arguments like Json<T>, State<T>, Path<T>
        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
            if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                if let Type::Path(inner_path) = inner_ty {
                    let inner_type = quote::quote!(#inner_path).to_string();

                    // Return the TypePath for common extractors
                    if ["Json", "State", "Path", "Query", "Form"].contains(&extractor.as_str()) {
                        return (extractor, inner_type, Some(inner_path.clone()));
                    }

                    return (extractor, inner_type, None);
                }
            }
        }

        // No generics (e.g., `Path<String>` where String is built-in)
        let full_type = quote::quote!(#type_path).to_string();
        (extractor, full_type, None)
    } else {
        // Malformed path
        let full = quote::quote!(#type_path).to_string();
        (full.clone(), full, None)
    }
}

fn build_params_array(parts: &[ParamInfoParts]) -> proc_macro2::TokenStream {
    let param_inits = parts.iter().map(|p| {
        let name = &p.name;
        let full_type = &p.full_type;
        let extractor = &p.extractor;
        let inner_type = &p.inner_type;

        let inner_type_lit = proc_macro2::Literal::string(inner_type);

        let schema_lookup = if let Some(inner_path) = &p.inner_type_path {
            let type_name = inner_path.path.segments.last().unwrap().ident.to_string();
            let type_name_lit = proc_macro2::Literal::string(&type_name);

            quote! {
                laz_types::find_type_schema(#type_name_lit)
                    .cloned()
                    .unwrap_or_else(|| laz_types::TypeSchema::Opaque(#inner_type_lit.to_string()))
            }
        } else {
            quote! {
                laz_types::TypeSchema::Primitive(#inner_type_lit.to_string())
            }
        };

        quote! {
            laz_types::ParamInfo {
                name: #name.to_string(),
                full_type: #full_type.to_string(),
                extractor: #extractor.to_string(),
                inner_type_schema: #schema_lookup,
            }
        }
    });

    quote! {
        vec![#(#param_inits),*]
    }
}
