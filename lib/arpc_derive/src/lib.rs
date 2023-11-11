#![feature(let_chains)]

use std::collections::HashMap;

use proc_macro2::{TokenStream, Span};
use syn::ExprLit;
use syn::{parse_macro_input, punctuated::Punctuated, TraitItem, FnArg, Ident, Type, TypeReference, Index, TypeParamBound, Signature, ReturnType, Pat, Path, ExprAssign, Expr, Lit, Token};
use syn::parse::{ParseStream, Parse, Result, Error};
use syn::spanned::Spanned;
use quote::{quote, quote_spanned, format_ident};
use convert_case::{Casing, Case};

struct ArpcMethod {
    wrapper_ident: Ident,
    client_async_signature: Signature,
    method_id: u32,
}

/// Checks if the given function is marked async or returns a impl future
// TODO: add an attribute that can be used to force a function to be run as async
// (for example if it returns a concrete type which is a future without using async or impl trait)
fn is_async(signature: &Signature) -> bool {
    if let ReturnType::Type(_, ret_type) = &signature.output {
        if let Type::ImplTrait(ret_type) = &**ret_type {
            return ret_type.bounds.iter().any(|t| {
                if let TypeParamBound::Trait(t) = t && let Some(t) = t.path.segments.first() {
                    t.ident.to_string() == "Future"
                } else {
                    false
                }
            });
        }
    }

    signature.asyncness.is_some()
}

/// Returns an ident for the name of the macro that will implement the client trait
fn client_impl_macro_name(trait_ident: &Ident) -> Ident {
    format_ident!("__arpc_impl_{}_async_client", trait_ident.to_string().to_case(Case::Snake))
}

/// Returns an ident for the name of the macro that will get the name of the client trait
fn client_resolve_macro_name(trait_ident: &Ident) -> Ident {
    format_ident!("__arpc_resolve_{}_async_client", trait_ident.to_string().to_case(Case::Snake))
}

struct Args {
    service_id: u64,
    /// Name used to generate clients
    name: String,
    supertrait_paths: HashMap<Ident, Path>,
}

impl Parse for Args {
    fn parse(input: ParseStream) -> Result<Self> {
        let args = Punctuated::<ExprAssign, Token!(,)>::parse_terminated(input)?;

        let mut service_id = None;
        let mut name = None;
        let mut supertrait_paths = HashMap::new();

        for arg in args.iter() {
            let Expr::Path(arg_name) = &*arg.left else {
                return Err(Error::new(arg.span(), "invalid argument name"));
            };

            let arg_ident = arg_name.path.require_ident()?;

            match arg_ident.to_string().as_str() {
                "service_id" => {
                    if service_id.is_some() {
                        return Err(Error::new(arg.span(), "service_id argument can only be specified once"));
                    }

                    let Expr::Lit(ExprLit { lit: Lit::Int(arg_value), .. }) = &*arg.right else {
                        return Err(Error::new(arg.span(), "invalid argument value for service_id"));
                    };

                    service_id = Some(arg_value.base10_parse()?);
                },
                "name" => {
                    if name.is_some() {
                        return Err(Error::new(arg.span(), "name argument can only be specified once"));
                    }

                    let Expr::Lit(ExprLit { lit: Lit::Str(arg_value), .. }) = &*arg.right else {
                        return Err(Error::new(arg.span(), "invalid argument value for name"));
                    };

                    name = Some(arg_value.value());
                },
                _ => {
                    // trait path is being specified
                    // TODO: maybe emit warning if path is being specified but it is not a supertrait

                    if supertrait_paths.contains_key(arg_ident) {
                        return Err(Error::new(arg.span(), format!("{} path already specified", arg_ident)));
                    }

                    let Expr::Path(trait_path) = &*arg.right else {
                        return Err(Error::new(arg.span(), "expected a path"));
                    };

                    supertrait_paths.insert(arg_ident.clone(), trait_path.path.clone());
                }
            }
        }

        Ok(Args {
            service_id: service_id.ok_or_else(|| input.error("service_id argument not specified"))?,
            name: name.ok_or_else(|| input.error("name argument not specified"))?,
            supertrait_paths,
        })
    }
}

#[proc_macro_attribute]
pub fn arpc_interface(args: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let args = parse_macro_input!(args as Args);
    let service_id = args.service_id;

    let input = parse_macro_input!(input as syn::ItemTrait);
    let trait_ident = input.ident;

    // otuput tokens
    let mut out = TokenStream::new();

    // tokens that should go inside of the trait
    let mut items = TokenStream::new();

    // methods for the async client
    let mut client_async_impls = TokenStream::new();

    // list of arpc methods
    let mut arpc_methods = Vec::new();

    for item in input.items.iter() {
        items.extend(quote! { #item });
        let TraitItem::Fn(fn_item) = item else {
            continue;
        };

        let signature = &fn_item.sig;
        let method_ident = &signature.ident;

        if let Some(unsafety) = signature.unsafety {
            out.extend(quote_spanned! {
                unsafety.span => compile_error!("arpc method must be safe");
            });
            continue;
        }

        let Some(reciever) = signature.receiver() else {
            out.extend(quote_spanned! {
                method_ident.span() => compile_error!("arpc method must have &self reciever");
            });
            continue;
        };

        let Type::Reference(TypeReference { mutability: None, .. }) = &*reciever.ty else {
            out.extend(quote_spanned! {
                reciever.self_token.span => compile_error("arpc method must have &self reciever")
            });
            continue;
        };

        // len makes ids sequentially assigned
        let method_id = arpc_methods.len() as u32;

        let fn_arg_types = signature.inputs.iter()
            .filter_map(|arg| {
                if let FnArg::Typed(arg) = arg {
                    Some(&*arg.ty)
                } else {
                    None
                }
            });
        
        let fn_arg_count = fn_arg_types.clone().count();
        
        let args_struct_ident = format_ident!("{}Args", signature.ident.to_string().to_case(Case::UpperCamel));

        out.extend(quote! {
            #[derive(serde::Serialize, serde::Deserialize)]
            struct #args_struct_ident(#(#fn_arg_types),*);
        });

        let method_wrapper_ident = format_ident!("{}_wrapper", signature.ident);

        let arg_struct_fields = (0..fn_arg_count).map(Index::from);

        if is_async(signature) {
            items.extend(quote! {
                fn #method_wrapper_ident(&self, data: &[u8], reply: sys::Reply) {
                    let Ok(message) = aser::from_bytes::<aurora::arpc::RpcCall<#args_struct_ident>>(data) else {
                        aurora::arpc::respond_error(reply, aurora::arpc::RpcError::SerializationError);
                        return;
                    };

                    aurora::async_runtime::spawn(async {
                        let result = #trait_ident::#method_ident(self, #(message.args.#arg_struct_fields),*).await;
                        aurora::arpc::respond_success(reply, result);
                    });
                }
            });
        } else {
            items.extend(quote! {
                fn #method_wrapper_ident(&self, data: &[u8], reply: sys::Reply) {
                    let Ok(message) = aser::from_bytes::<aurora::arpc::RpcCall<#args_struct_ident>>(data) else {
                        aurora::arpc::respond_error(reply, aurora::arpc::RpcError::SerializationError);
                        return;
                    };

                    let result = #trait_ident::#method_ident(self, #(message.args.#arg_struct_fields),*);
                    aurora::arpc::respond_success(reply, result);
                }
            });
        }

        let mut client_async_signature = signature.clone();
        client_async_signature.asyncness = Some(Token!(async)(Span::call_site()));
        let mut unnamed_arg_count = 0u32;
        let args = client_async_signature.inputs.iter()
            .filter_map(|arg| {
                if let FnArg::Typed(arg) = arg {
                    if let Pat::Ident(pat_ident) = &*arg.pat {
                        Some(pat_ident.ident.clone())
                    } else {
                        let ident = format_ident!("a{}", unnamed_arg_count);
                        unnamed_arg_count += 1;
                        Some(ident)
                    }
                } else {
                    None
                }
            });


        client_async_impls.extend(quote! {
            #client_async_signature {
                let args = #args_struct_ident(#(#args),*);
                let message = aurora::arpc::RpcCall {
                    service_id: #service_id,
                    method_id: #method_id,
                    args,
                };

                // TODO: make try_ version which does not panic when rpc fails
                self.endpoint().call(message).await.expect("failed to make rpc call")
            }
        });

        arpc_methods.push(ArpcMethod {
            wrapper_ident: method_wrapper_ident,
            client_async_signature,
            method_id,
        });
    }

    let trait_vis = input.vis;
    let method_ids = arpc_methods.iter()
        .map(|m| m.method_id);
    let wrapper_idents = arpc_methods.iter()
        .map(|m| &m.wrapper_ident);
    let supertraits = &input.supertraits;
    let arpc_supertraits_iter = supertraits.iter()
        .filter_map(|t| {
            if let TypeParamBound::Trait(t) = t {
                Some(&t.path)
            } else {
                None
            }
        });
    let supertrait_count = arpc_supertraits_iter.clone().count();
    let arpc_supertraits = arpc_supertraits_iter.clone();

    out.extend(quote! {
        #trait_vis trait #trait_ident: #supertraits {
            #items

            fn call_inner(&self, call_data: &aurora::arpc::RpcCallMethod, data: &[u8], reply_id: sys::CapId) -> bool {
                if call_data.service_id != #service_id {
                    #(
                        if #arpc_supertraits::call_inner(self, call_data, data, reply_id) {
                            return true;
                        }
                    )*

                    false
                } else {
                    let reply = sys::Reply::from_cap_id(reply_id).unwrap();
                    match call_data.method_id {
                        #(#method_ids => #trait_ident::#wrapper_idents(self, data, reply),)*
                        _ => aurora::arpc::respond_error(reply, aurora::arpc::RpcError::InvalidMethodId),
                    }

                    true
                }
            }

            fn call(&self, data: &[u8], reply: sys::Reply) {
                let Ok(call_data) = aser::from_bytes::<aurora::arpc::RpcCallMethod>(data) else {
                    aurora::arpc::respond_error(reply, aurora::arpc::RpcError::SerializationError);
                    return;
                };

                let cap_id = sys::Capability::cap_id(&reply);
                core::mem::forget(reply);

                if !#trait_ident::call_inner(self, &call_data, data, cap_id) {
                    let reply = sys::Reply::from_cap_id(cap_id).unwrap();
                    aurora::arpc::respond_error(reply, aurora::arpc::RpcError::InvalidServiceId);
                }
            }
        }
    });

    let client_struct_ident = format_ident!("{}", args.name);
    let client_async_trait = format_ident!("{}Async", args.name);
    let resolve_client_macro_ident = client_resolve_macro_name(&trait_ident);
    let impl_client_macro_ident = client_impl_macro_name(&trait_ident);

    let client_async_sigs = arpc_methods
        .iter()
        .map(|method| &method.client_async_signature);

    let supertrait_paths = arpc_supertraits_iter
        .clone()
        .map(|t| {
            let trait_ident = &t.segments.last().unwrap().ident;
            args.supertrait_paths.get(trait_ident)
                .expect("path to supetrait not specified")
        });
    let supertrait_paths2 = supertrait_paths.clone();

    // fixme: these are wrong, need macro to get them
    let supertrait_resolve_macros = arpc_supertraits_iter
        .clone()
        .map(|t| {
            let trait_ident = &t.segments.last().unwrap().ident;
            client_resolve_macro_name(trait_ident)
        });

    let supertrait_impl_macros = arpc_supertraits_iter
        .map(|t| {
            let trait_ident = &t.segments.last().unwrap().ident;
            client_impl_macro_name(trait_ident)
        });
    
    let supertrait_aliases = (0..supertrait_count)
        .map(|n| format_ident!("__arpc_{}_alias{}", trait_ident, n))
        .collect::<Vec<_>>();

    out.extend(quote! {
        #[derive(serde::Serialize, serde::Deserialize)]
        pub struct #client_struct_ident(aurora::arpc::ClientRpcEndpoint);

        impl #client_struct_ident {
            pub fn into_endpoint(self) -> aurora::arpc::ClientRpcEndpoint {
                self.0
            }

            pub fn endpoint(&self) -> &aurora::arpc::ClientRpcEndpoint {
                &self.0
            }
        }

        impl From<aurora::arpc::ClientRpcEndpoint> for #client_struct_ident {
            fn from(endpoint: aurora::arpc::ClientRpcEndpoint) -> Self {
                Self(endpoint)
            }
        }

        #(
            // generate aliases for client supertraits so we know what they are clled
            #supertrait_paths::#supertrait_resolve_macros!(#supertrait_aliases);
        )*

        pub trait #client_async_trait: #(#supertrait_aliases)+* {
            fn downcast(self) -> #client_struct_ident;

            #(#client_async_sigs;)*
        }

        pub macro #resolve_client_macro_ident($alias:ident) {
            trait $alias = #client_async_trait;
        }

        pub macro #impl_client_macro_ident($client_struct:ident) {
            #(#supertrait_paths2::#supertrait_impl_macros!($client_struct);)*

            impl #client_async_trait for $client_struct {
                fn downcast(self) -> #client_struct_ident {
                    #client_struct_ident(self.into_endpoint())
                }

                #client_async_impls
            }
        }

        #impl_client_macro_ident!(#client_struct_ident);
    });

    out.into()
}

#[proc_macro_attribute]
pub fn arpc_impl(_args: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as syn::ItemImpl);

    let impl_type = &input.self_ty;
    let arpc_trait = &input.trait_.as_ref().expect("not an arpc trait impl").1;

    quote! {
        #input

        impl aurora::arpc::RpcService for #impl_type {
            fn call(&self, data: &[u8], reply: sys::Reply) {
                #arpc_trait::call(self, data, reply);
            }
        }
    }.into()
}