//! cmx-macros — 权限/角色注解属性宏。
//!
//! 对标 Spring Security 注解体系，提供 7 个属性宏：
//! - 权限：`#[has_permission]` / `#[has_permissions]` / `#[has_any_permission]`
//! - 角色：`#[has_role]` / `#[has_roles]` / `#[has_any_role]`
//! - 公开：`#[permit_all]`
//!
//! 宏生成的 `inventory::submit!` 代码在调用方 crate 中展开，
//! 类型路径使用绝对路径 `::cmx_core::...` 和 `::inventory::submit!`。

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{
    Expr, Ident, ItemFn, LitStr, Pat, Token, parse::Parser, parse_macro_input,
    punctuated::Punctuated,
};

// =========================================================================
// 辅助函数
// =========================================================================

/// 在函数参数中查找 `CmxSvrContext` 类型的 binding。
///
/// 通过类型路径匹配（非参数名），匹配路径末段为 `CmxSvrContext` 的 TupleStruct 模式。
fn find_svr_ctx_binding(item_fn: &ItemFn) -> syn::Result<Ident> {
    for arg in &item_fn.sig.inputs {
        if let syn::FnArg::Typed(pat_type) = arg {
            // 检查是否为 TupleStruct 模式: CmxSvrContext(svr_ctx)
            if let Pat::TupleStruct(tuple_struct) = &*pat_type.pat {
                // 检查路径末段是否为 "CmxSvrContext"
                if let Some(last_seg) = tuple_struct.path.segments.last()
                    && last_seg.ident == "CmxSvrContext"
                {
                    // 提取内部 binding
                    if let Some(Pat::Ident(pat_ident)) = tuple_struct.elems.first() {
                        return Ok(pat_ident.ident.clone());
                    }
                }
            }
        }
    }
    Err(syn::Error::new(
        Span::call_site(),
        "权限注解宏要求函数包含 CmxSvrContext 类型参数(如 CmxSvrContext(svr_ctx): CmxSvrContext)",
    ))
}

/// 生成 `inventory::submit!` 注册 `RegisteredRouteHandler` 的代码。
fn gen_route_handler_registration(handler_name: &str, is_public: bool) -> proc_macro2::TokenStream {
    let handler_name_lit = LitStr::new(handler_name, Span::call_site());
    let source = format!("{}:{}", env!("CARGO_PKG_NAME"), handler_name);
    let source_lit = LitStr::new(&source, Span::call_site());
    quote! {
        ::inventory::submit! {
            ::cmx_core::model::iam::registry::RegisteredRouteHandler {
                handler_name: #handler_name_lit,
                is_public: #is_public,
                source: #source_lit,
            }
        }
    }
}

/// 解析逗号分隔的字符串字面量列表（如 `"a", "b"`）。
fn parse_str_list(args: TokenStream) -> syn::Result<Vec<LitStr>> {
    let punctuated: Punctuated<LitStr, Token![,]> =
        Punctuated::parse_terminated.parse2(args.into())?;
    Ok(punctuated.into_iter().collect())
}

// =========================================================================
// 1. #[has_permission] — 单权限检查(含元数据注册)
// =========================================================================

/// 受保护路由处理器权限注解（对标 Spring Security `hasAuthority`）。
///
/// 唯一带权限元数据注册的宏：
/// 1. `inventory::submit!` 注册 `RegisteredPermission`（key/group/display/description）
/// 2. `inventory::submit!` 注册 `RegisteredRouteHandler { is_public: false }`
/// 3. 在函数体首行注入 `require_permission` 检查
///
/// # Arguments
///
/// 属性参数使用 `key = "value"` 形式，逗号分隔：
/// * `key` - 权限码，全局唯一标识（如 `"user:create"`）。
/// * `group` - 权限分组，用于管理界面分类展示。
/// * `display` - 显示名称（中文友好）。
/// * `description` - 详细描述。
///
/// 被注解的函数必须包含 `CmxSvrContext(svr_ctx): CmxSvrContext` 参数，宏通过类型路径识别。
///
/// # Returns
///
/// 返回展开后的 `TokenStream`，包含原函数（函数体首行已注入权限检查）、
/// `RegisteredPermission` 注册代码和 `RegisteredRouteHandler` 登记代码。
/// 若函数缺少 `CmxSvrContext` 参数，返回编译错误。
///
/// # Examples
///
/// ```ignore
/// #[cmx_macros::has_permission(
///     key = "user:create",
///     group = "用户管理",
///     display = "创建用户",
///     description = "创建新用户账户"
/// )]
/// pub async fn create_user(...) -> ... { ... }
/// ```
#[proc_macro_attribute]
pub fn has_permission(args: TokenStream, input: TokenStream) -> TokenStream {
    let mut item_fn = parse_macro_input!(input as ItemFn);

    // 解析 key = "..." group = "..." display = "..." description = "..."
    let mut key = String::new();
    let mut group = String::new();
    let mut display = String::new();
    let mut description = String::new();

    if !args.is_empty() {
        let args_str = args.to_string();
        // 简单解析 key = "value" 格式
        for pair in args_str.split(',') {
            let pair = pair.trim();
            if let Some((k, v)) = pair.split_once('=') {
                let k = k.trim();
                let v = v.trim().trim_matches('"');
                match k {
                    "key" => key = v.to_string(),
                    "group" => group = v.to_string(),
                    "display" => display = v.to_string(),
                    "description" => description = v.to_string(),
                    _ => {}
                }
            }
        }
    }

    let binding = match find_svr_ctx_binding(&item_fn) {
        Ok(b) => b,
        Err(e) => return e.to_compile_error().into(),
    };

    let handler_name = item_fn.sig.ident.to_string();
    let key_lit = LitStr::new(&key, Span::call_site());
    let group_lit = LitStr::new(&group, Span::call_site());
    let display_lit = LitStr::new(&display, Span::call_site());
    let description_lit = LitStr::new(&description, Span::call_site());
    let source = format!("cmx-macros:{}", handler_name);
    let source_lit = LitStr::new(&source, Span::call_site());

    let route_handler_reg = gen_route_handler_registration(&handler_name, false);

    let perm_check = quote! {
        #binding.require_permission(#key_lit)
            .map_err(::cmx_api_types::Error::from)?;
    };

    // 注入到函数体首行
    let orig_block = &item_fn.block;
    *item_fn.block = syn::parse_quote! {
        {
            #perm_check
            #orig_block
        }
    };

    // 在函数外生成 inventory::submit!(静态注册)
    let expanded = quote! {
        #item_fn

        ::inventory::submit! {
            ::cmx_core::model::iam::registry::RegisteredPermission {
                key: #key_lit,
                group: #group_lit,
                display: #display_lit,
                description: #description_lit,
                source: #source_lit,
            }
        }

        #route_handler_reg
    };

    expanded.into()
}

// =========================================================================
// 2. #[has_permissions] — 全部权限检查(AND 语义)
// =========================================================================

/// 要求调用者拥有所有指定权限（对标 Spring Security 多个 `hasAuthority` 用 `and` 连接）。
///
/// 在函数体首行注入 `require_all_permissions` 检查（AND 语义），并通过
/// `inventory::submit!` 登记 `RegisteredRouteHandler { is_public: false }`。
/// 不注册权限元数据，权限码首次出现时建议使用 `#[has_permission]`。
///
/// # Arguments
///
/// 属性参数为逗号分隔的字符串字面量列表（如 `"a", "b"`）。
/// 被注解的函数必须包含 `CmxSvrContext(svr_ctx): CmxSvrContext` 参数。
///
/// # Returns
///
/// 返回展开后的 `TokenStream`，包含原函数（函数体首行已注入权限检查）
/// 和 `RegisteredRouteHandler` 登记代码。若参数解析失败或函数缺少
/// `CmxSvrContext` 参数，返回编译错误。
///
/// # Examples
///
/// ```ignore
/// #[cmx_macros::has_permissions("agent:export", "agent:read")]
/// pub async fn export_agent_detail(...) -> ... { ... }
/// ```
#[proc_macro_attribute]
pub fn has_permissions(args: TokenStream, input: TokenStream) -> TokenStream {
    let mut item_fn = parse_macro_input!(input as ItemFn);

    // 解析逗号分隔的权限码列表
    let keys = match parse_str_list(args) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    let binding = match find_svr_ctx_binding(&item_fn) {
        Ok(b) => b,
        Err(e) => return e.to_compile_error().into(),
    };

    let handler_name = item_fn.sig.ident.to_string();
    let route_handler_reg = gen_route_handler_registration(&handler_name, false);

    let keys_arr: Vec<Expr> = keys
        .iter()
        .map(|k| {
            Expr::Lit(syn::ExprLit {
                attrs: vec![],
                lit: syn::Lit::Str(k.clone()),
            })
        })
        .collect();

    let perm_check = quote! {
        #binding.require_all_permissions(&[#(#keys_arr),*])
            .map_err(::cmx_api_types::Error::from)?;
    };

    let orig_block = &item_fn.block;
    *item_fn.block = syn::parse_quote! {
        {
            #perm_check
            #orig_block
        }
    };

    let expanded = quote! {
        #item_fn
        #route_handler_reg
    };

    expanded.into()
}

// =========================================================================
// 3. #[has_any_permission] — 任一权限检查(OR 语义)
// =========================================================================

/// 要求调用者拥有任一指定权限即可（对标 Spring Security `hasAnyAuthority`）。
///
/// 在函数体首行注入 `require_any_permission` 检查（OR 语义），并通过
/// `inventory::submit!` 登记 `RegisteredRouteHandler { is_public: false }`。
/// 不注册权限元数据，权限码首次出现时建议使用 `#[has_permission]`。
///
/// # Arguments
///
/// 属性参数为逗号分隔的字符串字面量列表（如 `"a", "b"`）。
/// 被注解的函数必须包含 `CmxSvrContext(svr_ctx): CmxSvrContext` 参数。
///
/// # Returns
///
/// 返回展开后的 `TokenStream`，包含原函数（函数体首行已注入权限检查）
/// 和 `RegisteredRouteHandler` 登记代码。若参数解析失败或函数缺少
/// `CmxSvrContext` 参数，返回编译错误。
///
/// # Examples
///
/// ```ignore
/// #[cmx_macros::has_any_permission("report:view", "report:export")]
/// pub async fn view_report(...) -> ... { ... }
/// ```
#[proc_macro_attribute]
pub fn has_any_permission(args: TokenStream, input: TokenStream) -> TokenStream {
    let mut item_fn = parse_macro_input!(input as ItemFn);

    let keys = match parse_str_list(args) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    let binding = match find_svr_ctx_binding(&item_fn) {
        Ok(b) => b,
        Err(e) => return e.to_compile_error().into(),
    };

    let handler_name = item_fn.sig.ident.to_string();
    let route_handler_reg = gen_route_handler_registration(&handler_name, false);

    let keys_arr: Vec<Expr> = keys
        .iter()
        .map(|k| {
            Expr::Lit(syn::ExprLit {
                attrs: vec![],
                lit: syn::Lit::Str(k.clone()),
            })
        })
        .collect();

    let perm_check = quote! {
        #binding.require_any_permission(&[#(#keys_arr),*])
            .map_err(::cmx_api_types::Error::from)?;
    };

    let orig_block = &item_fn.block;
    *item_fn.block = syn::parse_quote! {
        {
            #perm_check
            #orig_block
        }
    };

    let expanded = quote! {
        #item_fn
        #route_handler_reg
    };

    expanded.into()
}

// =========================================================================
// 4. #[has_role] — 单角色检查
// =========================================================================

/// 要求调用者拥有指定角色（对标 Spring Security `hasRole`）。
///
/// 在函数体首行注入 `require_role` 检查，并通过 `inventory::submit!` 登记
/// `RegisteredRouteHandler { is_public: false }`。不注册权限元数据。
///
/// # Arguments
///
/// 属性参数为单个字符串字面量（如 `"admin"`）。
/// 被注解的函数必须包含 `CmxSvrContext(svr_ctx): CmxSvrContext` 参数。
///
/// # Returns
///
/// 返回展开后的 `TokenStream`，包含原函数（函数体首行已注入角色检查）
/// 和 `RegisteredRouteHandler` 登记代码。若参数不是字符串字面量或函数缺少
/// `CmxSvrContext` 参数，返回编译错误。
///
/// # Examples
///
/// ```ignore
/// #[cmx_macros::has_role("admin")]
/// pub async fn system_settings(...) -> ... { ... }
/// ```
#[proc_macro_attribute]
pub fn has_role(args: TokenStream, input: TokenStream) -> TokenStream {
    let mut item_fn = parse_macro_input!(input as ItemFn);

    let role: LitStr = parse_macro_input!(args);

    let binding = match find_svr_ctx_binding(&item_fn) {
        Ok(b) => b,
        Err(e) => return e.to_compile_error().into(),
    };

    let handler_name = item_fn.sig.ident.to_string();
    let route_handler_reg = gen_route_handler_registration(&handler_name, false);

    let perm_check = quote! {
        #binding.require_role(#role)
            .map_err(::cmx_api_types::Error::from)?;
    };

    let orig_block = &item_fn.block;
    *item_fn.block = syn::parse_quote! {
        {
            #perm_check
            #orig_block
        }
    };

    let expanded = quote! {
        #item_fn
        #route_handler_reg
    };

    expanded.into()
}

// =========================================================================
// 5. #[has_roles] — 全部角色检查(AND 语义)
// =========================================================================

/// 要求调用者拥有所有指定角色（对标 Spring Security 多个 `hasRole` 用 `and` 连接）。
///
/// 在函数体首行注入 `require_all_roles` 检查（AND 语义），并通过
/// `inventory::submit!` 登记 `RegisteredRouteHandler { is_public: false }`。
/// 不注册权限元数据。
///
/// # Arguments
///
/// 属性参数为逗号分隔的字符串字面量列表（如 `"admin", "auditor"`）。
/// 被注解的函数必须包含 `CmxSvrContext(svr_ctx): CmxSvrContext` 参数。
///
/// # Returns
///
/// 返回展开后的 `TokenStream`，包含原函数（函数体首行已注入角色检查）
/// 和 `RegisteredRouteHandler` 登记代码。若参数解析失败或函数缺少
/// `CmxSvrContext` 参数，返回编译错误。
///
/// # Examples
///
/// ```ignore
/// #[cmx_macros::has_roles("admin", "auditor")]
/// pub async fn audit_admin_op(...) -> ... { ... }
/// ```
#[proc_macro_attribute]
pub fn has_roles(args: TokenStream, input: TokenStream) -> TokenStream {
    let mut item_fn = parse_macro_input!(input as ItemFn);

    let roles = match parse_str_list(args) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    let binding = match find_svr_ctx_binding(&item_fn) {
        Ok(b) => b,
        Err(e) => return e.to_compile_error().into(),
    };

    let handler_name = item_fn.sig.ident.to_string();
    let route_handler_reg = gen_route_handler_registration(&handler_name, false);

    let roles_arr: Vec<Expr> = roles
        .iter()
        .map(|k| {
            Expr::Lit(syn::ExprLit {
                attrs: vec![],
                lit: syn::Lit::Str(k.clone()),
            })
        })
        .collect();

    let perm_check = quote! {
        #binding.require_all_roles(&[#(#roles_arr),*])
            .map_err(::cmx_api_types::Error::from)?;
    };

    let orig_block = &item_fn.block;
    *item_fn.block = syn::parse_quote! {
        {
            #perm_check
            #orig_block
        }
    };

    let expanded = quote! {
        #item_fn
        #route_handler_reg
    };

    expanded.into()
}

// =========================================================================
// 6. #[has_any_role] — 任一角色检查(OR 语义)
// =========================================================================

/// 要求调用者拥有任一指定角色即可（对标 Spring Security `hasAnyRole`）。
///
/// 在函数体首行注入 `require_any_role` 检查（OR 语义），并通过
/// `inventory::submit!` 登记 `RegisteredRouteHandler { is_public: false }`。
/// 不注册权限元数据。
///
/// # Arguments
///
/// 属性参数为逗号分隔的字符串字面量列表（如 `"admin", "manager"`）。
/// 被注解的函数必须包含 `CmxSvrContext(svr_ctx): CmxSvrContext` 参数。
///
/// # Returns
///
/// 返回展开后的 `TokenStream`，包含原函数（函数体首行已注入角色检查）
/// 和 `RegisteredRouteHandler` 登记代码。若参数解析失败或函数缺少
/// `CmxSvrContext` 参数，返回编译错误。
///
/// # Examples
///
/// ```ignore
/// #[cmx_macros::has_any_role("admin", "manager")]
/// pub async fn manage_team(...) -> ... { ... }
/// ```
#[proc_macro_attribute]
pub fn has_any_role(args: TokenStream, input: TokenStream) -> TokenStream {
    let mut item_fn = parse_macro_input!(input as ItemFn);

    let roles = match parse_str_list(args) {
        Ok(v) => v,
        Err(e) => return e.to_compile_error().into(),
    };

    let binding = match find_svr_ctx_binding(&item_fn) {
        Ok(b) => b,
        Err(e) => return e.to_compile_error().into(),
    };

    let handler_name = item_fn.sig.ident.to_string();
    let route_handler_reg = gen_route_handler_registration(&handler_name, false);

    let roles_arr: Vec<Expr> = roles
        .iter()
        .map(|k| {
            Expr::Lit(syn::ExprLit {
                attrs: vec![],
                lit: syn::Lit::Str(k.clone()),
            })
        })
        .collect();

    let perm_check = quote! {
        #binding.require_any_role(&[#(#roles_arr),*])
            .map_err(::cmx_api_types::Error::from)?;
    };

    let orig_block = &item_fn.block;
    *item_fn.block = syn::parse_quote! {
        {
            #perm_check
            #orig_block
        }
    };

    let expanded = quote! {
        #item_fn
        #route_handler_reg
    };

    expanded.into()
}

// =========================================================================
// 7. #[permit_all] — 公开访问标记
// =========================================================================

/// 标记路由为公开访问（对标 Spring Security `permitAll`），无需认证/权限。
///
/// 仅通过 `inventory::submit!` 登记 `RegisteredRouteHandler { is_public: true }`
/// 用于漏写统计，不注入任何鉴权代码，函数体保持原样。
///
/// # Returns
///
/// 返回展开后的 `TokenStream`，包含原函数和 `RegisteredRouteHandler` 登记代码
/// （`is_public: true`）。
///
/// # Examples
///
/// ```ignore
/// #[cmx_macros::permit_all]
/// pub async fn health_check() -> Json<Value> { ... }
/// ```
#[proc_macro_attribute]
pub fn permit_all(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item_fn = parse_macro_input!(input as ItemFn);
    let handler_name = item_fn.sig.ident.to_string();
    let route_handler_reg = gen_route_handler_registration(&handler_name, true);

    let expanded = quote! {
        #item_fn
        #route_handler_reg
    };

    expanded.into()
}
