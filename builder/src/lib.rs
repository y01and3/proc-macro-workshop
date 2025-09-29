use proc_macro::TokenStream;
use proc_macro2::{Ident, Span};
use quote::quote;
use syn::{parse_macro_input, Data, DeriveInput};

#[proc_macro_derive(Builder)]
pub fn derive(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let attrs = input.attrs;
    let vis = input.vis;
    let ident = input.ident;
    let new_ident = Ident::new(&format!("{}Builder", ident), Span::call_site());
    let generics = input.generics;
    if let Data::Struct(data) = input.data {
        let fields = data
            .fields
            .iter()
            .map(|field| {
                let attrs = &field.attrs;
                let vis = &field.vis;
                let ident = &field.ident;
                let ty = &field.ty;

                return quote! {
                    #(#attrs)*
                    #vis #ident: Option<#ty>,
                };
            })
            .collect::<Vec<proc_macro2::TokenStream>>();

        let data_fields = data
            .fields
            .iter()
            .map(|field| {
                let ident = &field.ident;

                return quote! {
                    #ident: None,
                };
            })
            .collect::<Vec<proc_macro2::TokenStream>>();

        let setters = data
            .fields
            .iter()
            .map(|field| {
                let ident = &field.ident;
                let ty = &field.ty;

                return quote! {
                    fn #ident(&mut self, #ident: #ty) -> &mut Self {
                        self.#ident = Some(#ident);
                        self
                    }
                };
            })
            .collect::<Vec<proc_macro2::TokenStream>>();

        let check_fields = data
            .fields
            .iter()
            .map(|field| {
                let ident = &field.ident;
                let ty = &field.ty;

                return quote! {
                    let #ident: #ty = self.#ident.as_ref().ok_or("Unexpected Null")?.clone();
                };
            })
            .collect::<Vec<proc_macro2::TokenStream>>();

        let checked_fields = data
            .fields
            .iter()
            .map(|field| {
                let ident = &field.ident;

                return quote! {
                    #ident,
                };
            })
            .collect::<Vec<proc_macro2::TokenStream>>();

        let expanded = quote! {
            #(#attrs)*
            #vis struct #new_ident #generics {
                #(#fields)*
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
                    })
                }
            }
        };

        TokenStream::from(expanded)
    } else {
        panic!()
    }
}
