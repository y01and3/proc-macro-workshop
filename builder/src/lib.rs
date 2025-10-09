use std::collections::HashMap;

use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, Attribute, Data, DeriveInput, Expr, GenericArgument, Lit, PathArguments,
    Type,
};

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
        let mut each_builder = HashMap::<Ident, Ident>::new(); // key is field name, value is function name

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

                if let Some(attr) = attrs.iter().find(|attr| attr.path().is_ident("builder")) {
                    let builder: Expr = attr.parse_args().unwrap();
                    match builder {
                        Expr::Assign(assign) => {
                            let left = assign.left.as_ref();
                            let right = assign.right.as_ref();
                            match (left, right) {
                                (Expr::Path(left), Expr::Lit(right)) => {
                                    if left.path.is_ident("each") {
                                        match &right.lit {
                                            Lit::Str(name) => {
                                                each_builder.insert(
                                                    ident.clone(),
                                                    format_ident!("{}", name.value()),
                                                );
                                            }
                                            _ => panic!("Expected `builder(each = \"...\")`"),
                                        }
                                    } else {
                                        panic!("Expected `builder(each = \"...\")`");
                                    }
                                }
                                _ => panic!("Expected `builder(each = \"...\")`"),
                            }
                        }
                        _ => panic!("Expected `builder(each = \"...\")`"),
                    }
                }
                let attrs = attrs
                    .iter()
                    .filter(|attr| !attr.path().is_ident("builder"))
                    .collect::<Vec<&Attribute>>();

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
                if each_builder
                    .keys()
                    .find(|ident| **ident == field.ident)
                    .is_some()
                {
                    quote! {#ident: Some(vec![]),}
                } else {
                    quote! {#ident: None,}
                }
            });

        let setters = build_fields
            .clone()
            .into_iter()
            .chain(raw_fields.clone().into_iter())
            .filter(|field| {
                each_builder
                    .keys()
                    .find(|ident| **ident == field.ident)
                    .is_none()
            })
            .map(|field| {
                let ident = &field.ident;
                let ty = &field.ty;

                quote! {
                    pub fn #ident(&mut self, #ident: #ty) -> &mut Self {
                        self.#ident = Some(#ident);
                        self
                    }
                }
            })
            .collect::<Vec<proc_macro2::TokenStream>>();

        let each_setters = build_fields
            .clone()
            .into_iter()
            .chain(raw_fields.clone().into_iter())
            .filter(|field| {
                each_builder
                    .keys()
                    .find(|ident| **ident == field.ident)
                    .is_some()
            })
            .map(|field| {
                let ident = &field.ident;
                let func = each_builder.get(ident).ok_or("Not find Ident").unwrap();
                let ty = &field.ty;

                let ty = match ty {
                    Type::Path(path) => path
                        .path
                        .segments
                        .last()
                        .and_then(|last| {
                            if last.ident.to_string() == "Vec" {
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
                        .ok_or("Expected Vec")
                        .unwrap(),
                    _ => panic!("Expected Vec"),
                };

                quote! {
                    pub fn #func(&mut self, #func: #ty) -> &mut Self{
                        match &mut self.#ident {
                            Some(vec) => vec.push(#func),
                            None=> self.#ident = Some(vec![#func]),
                        };
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

                #(#each_setters)*

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
