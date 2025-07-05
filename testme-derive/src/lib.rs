use proc_macro::TokenStream;
use proc_macro2::{Span, Ident};
use quote::quote;
use syn::{parse_macro_input, Item, ItemMod};

fn get_hash(s: &str) -> u32 {
    adler2::adler32_slice(s.as_bytes())
}

struct LinkmeIdents {
    before_each: Ident,
    before_all: Ident,
    after_each: Ident,
    after_all: Ident,
}

fn get_all_linkme_idents(hash: u32) -> LinkmeIdents {
    let before_each = proc_macro2::Ident::new(&format!("BEFORE_EACH_{}", hash), Span::call_site());
    let before_all = proc_macro2::Ident::new(&format!("BEFORE_ALL_{}", hash), Span::call_site());
    let after_each = proc_macro2::Ident::new(&format!("AFTER_EACH_{}", hash), Span::call_site());
    let after_all = proc_macro2::Ident::new(&format!("AFTER_ALL_{}", hash), Span::call_site());
    LinkmeIdents { before_each, before_all, after_each, after_all }
}

fn get_all_linkme_statics(linkme_idents: LinkmeIdents) -> [Item; 4] {
    let LinkmeIdents {
        before_each,
        before_all,
        after_each,
        after_all,
    } = linkme_idents;
    let before_each_item: Item = syn::parse_quote!(
        #[::testme::distributed_slice]
        static #before_each: [fn() -> std::pin::Pin<Box<dyn Future<Output=()> + Send>>];
    );
    let before_all_item: Item = syn::parse_quote!(
        #[::testme::distributed_slice]
        static #before_all: [fn() -> std::pin::Pin<Box<dyn Future<Output = ()>>>];
    );
    let after_each_item: Item = syn::parse_quote!(
        #[::testme::distributed_slice]
        static #after_each: [fn() -> std::pin::Pin<Box<dyn Future<Output=()> + Send>>];
    );
    let after_all_item: Item = syn::parse_quote!(
        #[::testme::distributed_slice]
        static #after_all: [fn() -> std::pin::Pin<Box<dyn Future<Output = ()>>>];
    );
    [before_each_item, before_all_item, after_each_item, after_all_item]
}

#[proc_macro_attribute]
pub fn testme(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input = parse_macro_input!(item as ItemMod);

    let mod_content = &mut input.content;

    // first, collect all function names, and create a hash from that
    // so we emit unique static linkme items that dont conflict with other
    // test modules where testme is used
    let mut all_fn_names = "".to_string();
    if let Some((_brace, items)) = mod_content {
        for item in items {
            if let Item::Fn(f) = item {
                all_fn_names.push_str(&f.sig.ident.to_string());
            }
        }
        // let item: Item = syn::parse_quote!(
        //     const X: u32 = 2;
        // );
        // items.push(item);
    }
    let hash = get_hash(&all_fn_names);
    let linkme_idents = get_all_linkme_idents(hash);
    let insert_items = get_all_linkme_statics(linkme_idents);
    if let Some((_, items)) = mod_content {
        for insert in insert_items {
            items.insert(0, insert);
        }
    }

    let output = quote! {
        #input
    };

    output.into()
}
