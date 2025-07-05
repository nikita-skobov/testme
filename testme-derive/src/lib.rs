use proc_macro::TokenStream;
use proc_macro2::{Span, Ident};
use quote::quote;
use syn::{parse_macro_input, Block, Item, ItemFn, ItemMod, Stmt};

fn get_hash(s: &str) -> u32 {
    adler2::adler32_slice(s.as_bytes())
}

#[derive(Clone)]
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

fn get_submit_test_internal_fn(linkme_idents: LinkmeIdents) -> Item {
    let LinkmeIdents {
        before_each,
        before_all,
        after_each,
        after_all,
    } = linkme_idents;
    let submit_internal: Item = syn::parse_quote!(
        fn submit_test_internal(
            rx: ::tokio::sync::oneshot::Receiver<Result<(), ::tokio::task::JoinError>>,
            t: ::testme::Test,
        ) {
            ::testme::submit_test(
                &RUN_ALL_LOCK,
                &TEST_HANDLES,
                #before_each,
                #before_all,
                #after_each,
                #after_all,
                rx,
                t,
            )
        }
    );
    submit_internal
}

fn get_all_static_items(linkme_idents: LinkmeIdents) -> [Item; 7] {
    let submit_fn = get_submit_test_internal_fn(linkme_idents.clone());

    let LinkmeIdents {
        before_each,
        before_all,
        after_each,
        after_all,
    } = linkme_idents;

    let test_handles: Item = syn::parse_quote!(
        static TEST_HANDLES: std::sync::OnceLock<std::sync::Mutex<Vec<::testme::Test>>> = std::sync::OnceLock::new();
    );
    let run_all_lock: Item = syn::parse_quote!(
        static RUN_ALL_LOCK: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    );

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
    [test_handles, run_all_lock, before_each_item, before_all_item, after_each_item, after_all_item, submit_fn]
}

fn is_test_fn(f: &ItemFn) -> bool {
    for attr in f.attrs.iter() {
        match &attr.meta {
            syn::Meta::Path(path) => {
                if let Some(i) = path.get_ident() {
                    if i.to_string() == "test" {
                        return true;
                    }
                }
            }
            _ => return false,
        }
    }
    false
}

fn wrap_test_fn(f: &mut ItemFn) {
    if !is_test_fn(f) {
        return;
    }
    if f.sig.asyncness.take().is_some() {
        wrap_test_fn_block_async(&mut f.block);
    } else {
        wrap_test_fn_block(&mut f.block);
    }
}

/// we consider any function with names:
/// - after_all
/// - before_all
/// - after_each
/// - before_each
/// 
/// to be a linkme function
fn wrap_linkme_functions(linkme_idents: &LinkmeIdents, f: &mut ItemFn) {
    let fn_name = f.sig.ident.to_string();
    match fn_name.as_str() {
        "before_all" => {
            wrap_linkme_func(linkme_idents.before_all.clone(), false, f);
        }
        "before_each" => {
            wrap_linkme_func(linkme_idents.before_each.clone(), true, f);
        }
        "after_all" => {
            wrap_linkme_func(linkme_idents.after_all.clone(), false, f);
        }
        "after_each" => {
            wrap_linkme_func(linkme_idents.after_each.clone(), true, f);
        }
        _ => return
    }
}

fn wrap_linkme_func(
    linkme_static: Ident,
    needs_send: bool,
    f: &mut ItemFn
) {
    let name = &f.sig.ident;
    let block = &f.block;

    let item_fn: ItemFn = if needs_send {
        syn::parse_quote!(
            #[::testme::distributed_slice(#linkme_static)]
            fn #name() -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>> {
                Box::pin(async #block)
            }
        )
    } else {
        syn::parse_quote!(
            #[::testme::distributed_slice(#linkme_static)]
            fn #name() -> std::pin::Pin<Box<dyn Future<Output = ()>>> {
                Box::pin(async #block)
            }
        )
    };
    *f = item_fn;
}

fn wrap_test_fn_block(b: &mut Block) {
    let stmts: Vec<Stmt> = syn::parse_quote!(
        let (tx, rx) = ::tokio::sync::oneshot::channel();

        let f = async {};
        let box_fut: std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>> = Box::pin(f);
        let blocking_fn = || #b;    
        let blocking_fn = Box::new(blocking_fn);

        submit_test_internal(rx, ::testme::Test { fut: box_fut, blocking_fn: Some(blocking_fn), callback: tx });
    );

    b.stmts = stmts;
}

fn wrap_test_fn_block_async(b: &mut Block) {
    let stmts: Vec<Stmt> = syn::parse_quote!(
        let (tx, rx) = ::tokio::sync::oneshot::channel();

        let f = async #b;
        let box_fut: std::pin::Pin<Box<dyn Future<Output = ()> + Send + Sync>> = Box::pin(f);

        submit_test_internal(rx, ::testme::Test { fut: box_fut, blocking_fn: None, callback: tx });
    );

    b.stmts = stmts;
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
                // modify in place all the test functions
                wrap_test_fn(f);
            }
        }
    }
    let hash = get_hash(&all_fn_names);
    let linkme_idents = get_all_linkme_idents(hash);
    let mut insert_items = get_all_static_items(linkme_idents.clone());
    insert_items.reverse();
    if let Some((_, items)) = mod_content {
        // now wrap any of the after/before each/all functions
        for item in items.iter_mut() {
            if let Item::Fn(f) = item {
                wrap_linkme_functions(&linkme_idents, f);
            }
        }
        // for aesthetic purposes, insert our generated code at the top:
        for insert in insert_items {
            items.insert(0, insert);
        }
    }

    let output = quote! {
        #input
    };

    output.into()
}
