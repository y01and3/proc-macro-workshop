use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{format_ident, quote};
use syn::{parse_macro_input, Data, DeriveInput, GenericArgument, PathArguments, Type};

#[derive(Clone)]
struct BuilderField {
    ident: Ident,
    ty: Type,
}

#[proc_macro_derive(Builder, attributes(builder))]
pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let attrs = input.attrs;
    let vis = input.vis;
    let ident = input.ident;
    let new_ident = format_ident!("{}Builder", ident);
    let generics = input.generics;

    if let Data::Struct(data) = input.data {
        let mut build_fields: Vec<BuilderField> = vec![];
        let mut raw_fields: Vec<BuilderField> = vec![];

        let struct_fields = data
            .fields
            .iter()
            .map(|field| {
                let attrs = &field.attrs;
                let vis = &field.vis;
                let ident = &field.ident;
                let ty = &field.ty;

                let ident = ident.as_ref().ok_or("Field no Ident").unwrap();
                let ty = match ty {
                    Type::Path(path) => path
                        .path
                        .segments
                        .last()
                        .and_then(|last| {
                            if last.ident.to_string() == "Option" {
                                match &last.arguments {
                                    PathArguments::AngleBracketed(inner) => {
                                        inner.args.first().and_then(|arg| match arg {
                                            GenericArgument::Type(ty) => Some(ty),
                                            _ => None,
                                        })
                                    }
                                    _ => None,
                                }
                            } else {
                                None
                            }
                        })
                        .map_or_else(
                            || {
                                build_fields.push(BuilderField {
                                    ident: ident.clone(),
                                    ty: ty.clone(),
                                });
                                ty
                            },
                            |ty| {
                                raw_fields.push(BuilderField {
                                    ident: ident.clone(),
                                    ty: ty.clone(),
                                });
                                ty
                            },
                        ),
                    _ => {
                        build_fields.push(BuilderField {
                            ident: ident.clone(),
                            ty: ty.clone(),
                        });
                        ty
                    }
                };

                quote! {
                    #(#attrs)*
                    #vis #ident: Option<#ty>,
                }
            })
            .collect::<Vec<proc_macro2::TokenStream>>();

        let data_fields = build_fields
            .clone()
            .into_iter()
            .chain(raw_fields.clone().into_iter())
            .map(|field| {
                let ident = &field.ident;
                quote! {#ident: None,}
            });

        let setters = build_fields
            .clone()
            .into_iter()
            .chain(raw_fields.clone().into_iter())
            .map(|field| {
                let ident = &field.ident;
                let ty = &field.ty;

                quote! {
                    fn #ident(&mut self, #ident: #ty) -> &mut Self {
                        self.#ident = Some(#ident);
                        self
                    }
                }
            })
            .collect::<Vec<proc_macro2::TokenStream>>();

        let check_fields = build_fields
            .clone()
            .into_iter()
            .map(|field| {
                let ident = &field.ident;
                let ty = &field.ty;

                quote! {
                    let #ident: #ty = self.#ident.as_ref().ok_or("Unexpected None")?.clone();
                }
            })
            .collect::<Vec<proc_macro2::TokenStream>>();

        let checked_fields = build_fields
            .clone()
            .into_iter()
            .map(|field| {
                let ident = &field.ident;

                quote! {
                    #ident,
                }
            })
            .collect::<Vec<proc_macro2::TokenStream>>();

        let unchanged_fields = raw_fields.clone().into_iter().map(|field| {
            let ident = &field.ident;

            quote! {
                #ident: self.#ident.clone(),
            }
        });

        let expanded = quote! {
            #(#attrs)*
            #vis struct #new_ident #generics {
                #(#struct_fields)*
            }

            impl #ident {
                pub fn builder() -> #new_ident {
                    #new_ident {
                        #(#data_fields)*
                    }
                }
            }

            impl #new_ident {
                #(#setters)*

                pub fn build(&mut self) -> Result<#ident, String> {
                    #(#check_fields)*

                    Ok(#ident {
                        #(#checked_fields)*
                        #(#unchanged_fields)*
                    })
                }
            }
        };

        TokenStream::from(expanded)
    } else {
        panic!()
    }
}
