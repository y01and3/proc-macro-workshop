use std::collections::HashMap;

use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, spanned::Spanned, Attribute, Data, DataStruct, DeriveInput, Error, Expr,
    GenericArgument, Lit, PathArguments, Type,
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
    let data_struct: DataStruct;

    match input.data {
        Data::Struct(data) => data_struct = data,
        Data::Enum(data) => {
            return TokenStream::from(
                Error::new(data.enum_token.span(), "expected struct").into_compile_error(),
            )
        }
        Data::Union(data) => {
            return TokenStream::from(
                Error::new(data.union_token.span(), "expected struct").into_compile_error(),
            )
        }
    }

    let mut build_fields: Vec<BuilderField> = vec![];
    let mut raw_fields: Vec<BuilderField> = vec![];
    let mut each_builder = HashMap::<Ident, Ident>::new(); // key is field name, value is function name
    let mut err: Option<Error> = None;

    let struct_fields = data_struct
        .fields
        .iter()
        .map(|field| {
            let attrs = &field.attrs;
            let vis = &field.vis;
            let ident = &field.ident;
            let ty = &field.ty;

            let ident = match ident.as_ref() {
                Some(ident) => ident,
                None => {
                    match &mut err {
                        Some(err) => err.combine(Error::new(field.span(), "No Ident")),
                        None => err = Some(Error::new(field.span(), "No Ident")),
                    };
                    return quote! {};
                }
            };
            
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

            match get_builder_attr(ident, attrs) {
                Ok(Some(set)) => {
                    each_builder.insert(set.0, set.1);
                }
                Err(new_err) => match &mut err {
                    Some(err) => err.combine(new_err),
                    None => err = Some(new_err),
                },
                _ => (),
            }

            let attrs = attrs
                .iter()
                .filter(|attr| !attr.path().is_ident("builder"))
                .collect::<Vec<&Attribute>>();

            quote! {
                #(#attrs)*
                #vis #ident: core::option::Option<#ty>,
            }
        })
        .collect::<Vec<proc_macro2::TokenStream>>();

    if let Some(err) = err {
        return TokenStream::from(err.into_compile_error());
    }

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
                quote! {#ident: core::option::Option::Some(vec![]),}
            } else {
                quote! {#ident: core::option::Option::None,}
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
                    self.#ident = core::option::Option::Some(#ident);
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
                let mut ty = &field.ty;

                match check_type_vec(ty) {
                    Ok(inner_ty) => ty = inner_ty,
                    Err(new_err) => match &mut err {
                        Some(err) => err.combine(new_err),
                        None => err = Some(new_err),
                    },
                }

                quote! {
                    pub fn #func(&mut self, #func: #ty) -> &mut Self{
                        match &mut self.#ident {
                            core::option::Option::Some(vec) => vec.push(#func),
                            core::option::Option::None=> self.#ident = core::option::Option::Some(vec![#func]),
                        };
                        self
                    }
                }
            })
            .collect::<Vec<proc_macro2::TokenStream>>();

    if let Some(err) = err {
        return TokenStream::from(err.into_compile_error());
    }

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

            pub fn build(&mut self) -> std::result::Result<#ident, String> {
                #(#check_fields)*

                std::result::Result::Ok(#ident {
                    #(#checked_fields)*
                    #(#unchanged_fields)*
                })
            }
        }
    };

    TokenStream::from(expanded)
}

fn get_builder_attr(
    ident: &Ident,
    attrs: &Vec<Attribute>,
) -> Result<Option<(Ident, Ident)>, Error> {
    if let Some(attr) = attrs.iter().find(|attr| attr.path().is_ident("builder")) {
        let builder: Expr = attr.parse_args()?;
        match builder {
            Expr::Assign(assign) => {
                let left = assign.left.as_ref();
                let right = assign.right.as_ref();
                match (left, right) {
                    (Expr::Path(left), Expr::Lit(right)) => {
                        if left.path.is_ident("each") {
                            match &right.lit {
                                Lit::Str(name) => {
                                    Ok(Some((ident.clone(), format_ident!("{}", name.value()))))
                                }
                                _ => Err(Error::new(
                                    attr.meta.span(),
                                    "expected `builder(each = \"...\")`",
                                )),
                            }
                        } else {
                            Err(Error::new(
                                attr.meta.span(),
                                "expected `builder(each = \"...\")`",
                            ))
                        }
                    }
                    _ => Err(Error::new(
                        attr.meta.span(),
                        "expected `builder(each = \"...\")`",
                    )),
                }
            }
            _ => Err(Error::new(
                attr.meta.span(),
                "expected `builder(each = \"...\")`",
            )),
        }
    } else {
        Ok(None)
    }
}

fn check_type_vec(ty: &Type) -> Result<&Type, Error> {
    match ty {
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
            .ok_or_else(|| Error::new(ty.span(), "expected Vector")),
        _ => Err(Error::new(ty.span(), "expected Vector")),
    }
}
