#![feature(let_chains)]

use proc_macro2::{TokenStream, Span};
use syn::{parse_macro_input, TraitItem, FnArg, Ident, Type, TypeReference, Index, TypeParamBound, Signature, ReturnType};
use quote::{quote, quote_spanned};

struct ArpcMethod {
    wrapper_ident: Ident,
    method_id: u32,
}

fn concat_ident(orig_ident: &Ident, data: &str) -> Ident {
    Ident::new(&format!("{}{}", orig_ident, data), Span::call_site())
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

#[proc_macro_attribute]
pub fn arpc_interface(args: proc_macro::TokenStream, input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // TODO: change so the syntax is `service_id = 10`
    let service_id_arg = parse_macro_input!(args as syn::LitInt);
    let service_id = service_id_arg.base10_parse::<u64>()
        .expect("could not parse service id");

    let input = parse_macro_input!(input as syn::ItemTrait);
    let trait_ident = input.ident;

    // otuput tokens
    let mut out = TokenStream::new();

    // tokens that should go inside of the trait
    let mut items = TokenStream::new();

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

        let fn_arg_types = signature.inputs.iter()
            .filter_map(|arg| {
                if let FnArg::Typed(arg) = arg {
                    Some(&*arg.ty)
                } else {
                    None
                }
            });
        
        let fn_arg_count = fn_arg_types.clone().count();
        
        let args_struct_ident = concat_ident(&signature.ident, "Args");
        let message_struct_ident = concat_ident(&signature.ident, "Message");

        out.extend(quote! {
            #[derive(Serialize, Deserialize)]
            struct #args_struct_ident(#(#fn_arg_types),*);
        });

        out.extend(quote! {
            #[derive(Serialize, Deserialize)]
            struct #message_struct_ident {
                args: #args_struct_ident,
            }
        });

        let method_wrapper_ident = concat_ident(&signature.ident, "_wrapper");

        let arg_struct_fields = (0..fn_arg_count).map(Index::from);

        if is_async(signature) {
            items.extend(quote! {
                fn #method_wrapper_ident(&self, data: &[u8], reply: sys::Reply) {
                    let Ok(message) = aser::from_bytes::<#message_struct_ident>(data) else {
                        // FIXME: reply with error
                        return;
                    };

                    aurora::async_runtime::spawn(async {
                        let result = #trait_ident::#method_ident(self, #(message.args.#arg_struct_fields),*).await;
                        // FIXME: write to reply
                    });
                }
            });
        } else {
            items.extend(quote! {
                fn #method_wrapper_ident(&self, data: &[u8], reply: sys::Reply) {
                    let Ok(message) = aser::from_bytes::<#message_struct_ident>(data) else {
                        // FIXME: reply with error
                        return;
                    };

                    let result = #trait_ident::#method_ident(self, #(message.args.#arg_struct_fields),*);
                    // FIXME: write to reply
                }
            });
        }

        arpc_methods.push(ArpcMethod {
            wrapper_ident: method_wrapper_ident,
            // len makes ids sequentially assigned
            method_id: arpc_methods.len() as u32,
        });
    }

    let trait_vis = input.vis;
    let method_ids = arpc_methods.iter()
        .map(|m| m.method_id);
    let wrapper_idents = arpc_methods.iter()
        .map(|m| &m.wrapper_ident);
    let supertraits = &input.supertraits;
    let arpc_supertraits = supertraits.iter()
        .filter_map(|t| {
            if let TypeParamBound::Trait(t) = t {
                Some(&t.path)
            } else {
                None
            }
        });

    out.extend(quote! {
        #trait_vis trait #trait_ident: #supertraits {
            #items

            fn call_inner(&self, call_data: &aurora::arpc::RpcCallData, data: &[u8], reply_id: sys::CapId) -> bool {
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
                        #(#method_ids => #trait_ident::#wrapper_idents(self, data, reply)),*,
                        // FIXME: send error about bad method id
                        _ => (),
                    }

                    true
                }
            }

            fn call(&self, data: &[u8], reply: sys::Reply) {
                let Ok(call_data) = aser::from_bytes::<aurora::arpc::RpcCallData>(data) else {
                    // FIXME: respond with error
                    return;
                };

                let cap_id = sys::Capability::cap_id(&reply);
                core::mem::forget(reply);

                if #trait_ident::call_inner(self, &call_data, data, cap_id) {
                    let reply = sys::Reply::from_cap_id(cap_id).unwrap();
                    // FIXME: send error, incorrect service id
                }
            }
        }
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