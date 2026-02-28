use darling::{FromDeriveInput, FromField, FromMeta};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::{parse_macro_input, spanned::Spanned, Data, DeriveInput, Fields};

#[derive(FromDeriveInput)]
#[darling(attributes(optional), forward_attrs(allow, doc, cfg, diesel))]
struct DeriveInputOption {
    #[darling(default)]
    name: Option<String>,
    #[darling(default)]
    derive: Option<String>,
    attrs: Vec<syn::Attribute>,
}

#[derive(FromField)]
#[darling(attributes(optional), forward_attrs(allow, doc, cfg, diesel))]
struct FieldOption {
    #[darling(default)]
    name: Option<String>,
    #[darling(default)]
    skip: bool,
    #[darling(default)]
    required: bool,
    attrs: Vec<syn::Attribute>,
}

#[proc_macro_derive(Optional, attributes(optional))]
pub fn derive_options(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = parse_macro_input!(input as DeriveInput);

    let new_struct = new_struct(&input);
    let expanded = quote! {
        #new_struct
    };

    proc_macro::TokenStream::from(expanded)
}

fn new_struct(input: &DeriveInput) -> TokenStream {
    let option = DeriveInputOption::from_derive_input(&input).unwrap();
    let name = &input.ident;
    let attrs = &option.attrs;
    let vis = &input.vis;
    let generics = &input.generics;

    let derive: Option<&String> = option.derive.as_ref();
    let derives: Vec<syn::Ident> = derive
        .map(|s| s.split(",").map(|s| s.trim()).filter(|s| !s.is_empty()).collect())
        .unwrap_or(vec![])
        .iter()
        .map(|s| syn::Ident::from_string(s).unwrap())
        .collect();

    let new_name = option.name.map(|n| syn::Ident::from_string(&n).unwrap()).unwrap_or(format_ident!("{}Opt", name));
    let new_fields = process_new_struct_fields(&input.data);
    let impl_from = impl_from(&input);

    quote! {
        #[derive(#(#derives, )*)]
        #(#attrs)*
        #vis struct #new_name #generics {
            #new_fields
        }

        #impl_from
    }
}

fn process_new_struct_fields(data: &Data) -> TokenStream {
    match data {
        Data::Struct(ref data) => match data.fields {
            Fields::Named(ref fields) => {
                let new_fields = fields
                    .named
                    .iter()
                    .map(|f| {
                        let option = FieldOption::from_field(f).unwrap();
                        if option.skip {
                            return None;
                        }
                        let vis = &f.vis;
                        let attrs = &option.attrs;
                        let name = &option.name.map(|n| syn::Ident::from_string(&n).unwrap());
                        let name = name.as_ref().or(f.ident.as_ref());
                        let ty = &f.ty;
                        let new_ty = if option.required {
                            quote! {#ty}
                        } else {
                            quote! {::std::option::Option<#ty>}
                        };
                        Some(quote_spanned! {f.span()=>
                            #(#attrs)*
                            #vis #name: #new_ty
                        })
                    })
                    .filter_map(|x| x);
                quote! {
                    #(#new_fields ,)*
                }
            }
            Fields::Unnamed(ref fields) => {
                let new_fields = fields
                    .unnamed
                    .iter()
                    .map(|f| {
                        let option = FieldOption::from_field(f).unwrap();
                        if option.skip {
                            return None;
                        }
                        let vis = &f.vis;
                        let attrs = &f.attrs;
                        let ty = &f.ty;
                        let new_ty = if option.required {
                            quote! {#ty}
                        } else {
                            quote! {::std::option::Option<#ty>}
                        };
                        Some(quote_spanned! {f.span()=>
                            #(#attrs)*
                            #vis #new_ty
                        })
                    })
                    .filter_map(|x| x);
                quote! {
                    #(#new_fields ,)*
                }
            }
            Fields::Unit => {
                quote! {}
            }
        },
        Data::Enum(_) | Data::Union(_) => unimplemented!(),
    }
}

fn impl_from(input: &DeriveInput) -> TokenStream {
    let option = DeriveInputOption::from_derive_input(&input).unwrap();
    let generics = &input.generics;
    let name = &input.ident;
    let (impl_g, ty_g, where_c) = generics.split_for_impl();
    let new_name = option.name.map(|n| syn::Ident::from_string(&n).unwrap()).unwrap_or(format_ident!("{}Opt", name));
    let fields = process_impl_from_fields(&input.data);
    quote! {
        impl #impl_g From< #name #generics > for #new_name #ty_g #where_c {
            fn from(src: #name #generics) -> Self {
                #new_name {
                    #fields
                }
            }
        }
    }
}

fn process_impl_from_fields(data: &Data) -> TokenStream {
    match data {
        Data::Struct(ref data) => match data.fields {
            Fields::Named(ref fields) => {
                let new_fields = fields
                    .named
                    .iter()
                    .map(|f| {
                        let option = FieldOption::from_field(f).unwrap();
                        if option.skip {
                            return None;
                        }
                        let old_ident = f.ident.as_ref();
                        let name = &option.name.map(|n| syn::Ident::from_string(&n).unwrap());
                        let name = name.as_ref().or(old_ident);
                        if option.required {
                            Some(quote_spanned! {f.span()=>
                                #name: src.#old_ident
                            })
                        } else {
                            Some(quote_spanned! {f.span()=>
                                #name: ::std::option::Option::Some(src.#old_ident)
                            })
                        }
                    })
                    .filter_map(|x| x);
                quote! {
                    #(#new_fields ,)*
                }
            }
            Fields::Unnamed(ref fields) => {
                let new_fields = fields
                    .unnamed
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        let option = FieldOption::from_field(f).unwrap();
                        if option.skip {
                            return None;
                        }
                        if option.required {
                            Some(quote_spanned! {f.span()=>
                                src.#i
                            })
                        } else {
                            Some(quote_spanned! {f.span()=>
                                ::std::option::Option::Some(src.#i)
                            })
                        }
                    })
                    .filter_map(|x| x);
                quote! {
                    #(#new_fields ,)*
                }
            }
            Fields::Unit => {
                quote! {}
            }
        },
        Data::Enum(_) | Data::Union(_) => unimplemented!(),
    }
}
