//! E2E 测试共享模块。
//!
//! 提供 HTTP 客户端、统一响应解析、登录引导与唯一数据生成。
//! 测试连接到正在运行的 `web-server`（默认 http://127.0.0.1:8080）。

use serde_json::Value;
use std::time::Duration;

/// 统一解析后的 API 响应。
///
/// 服务端所有响应（成功或失败）均为 `{"code":u16,"msg":String,...}` 结构：
/// - 成功：HTTP 200，`code == 0`
/// - 业务错误（`ApiResp::fail`）：HTTP 200，`code != 0`
/// - `Err(Error)` 错误：HTTP 4xx/5xx，body 仍含 `code`/`msg`
pub struct ApiResult {
    /// HTTP 状态码。
    pub status: u16,
    /// 业务码（0 表示成功）。
    pub code: u16,
    /// 响应消息。
    pub msg: String,
    /// 业务数据。
    pub data: Option<Value>,
    /// 分页信息。
    #[allow(dead_code)]
    pub pagination: Option<Value>,
    /// 原始响应体。
    #[allow(dead_code)]
    pub raw: Value,
}

impl ApiResult {
    /// 断言成功：业务码为 0，并返回 data。
    pub fn assert_success(self) -> Value {
        assert_eq!(
            self.code, 0,
            "期望成功(code=0)，实际 code={} msg={} status={}",
            self.code, self.msg, self.status
        );
        self.data.unwrap_or_else(|| {
            panic!("成功响应缺少 data 字段；msg={}", self.msg);
        })
    }

    /// 断言业务错误：业务码非 0（与期望码匹配时更严格）。
    ///
    /// 由于错误可能经 `ApiResp::fail`（HTTP 200）或 `Err(Error)`（HTTP 4xx）返回，
    /// 此处只校验业务码非 0；如指定 `expected_code` 则同时校验相等。
    #[allow(dead_code)]
    pub fn assert_error(self, expected_code: Option<u16>) -> Value {
        assert_ne!(
            self.code, 0,
            "期望失败，实际成功；status={} msg={}",
            self.status, self.msg
        );
        if let Some(exp) = expected_code {
            assert_eq!(
                self.code, exp,
                "期望业务码 {}，实际 {} msg={} status={}",
                exp, self.code, self.msg, self.status
            );
        }
        self.data.unwrap_or(Value::Null)
    }
}

/// 从 JSON 对象中按 key 取字符串值，兼容 snake_case / camelCase。
pub fn get_str(obj: &Value, key: &str) -> Option<String> {
    flex_get(obj, key)
        .as_ref()
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// 从 JSON 对象中按 key 取值，兼容 snake_case / camelCase。
///
/// 优先尝试原 key，再尝试大小写变体，避免序列化约定差异导致误判。
pub fn flex_get(obj: &Value, key: &str) -> Option<Value> {
    if let Some(v) = obj.get(key) {
        return Some(v.clone());
    }
    // snake_case -> camelCase
    let camel = snake_to_camel(key);
    if camel != key
        && let Some(v) = obj.get(&camel) {
            return Some(v.clone());
        }
    // camelCase -> snake_case
    let snake = camel_to_snake(key);
    if snake != key
        && let Some(v) = obj.get(&snake) {
            return Some(v.clone());
        }
    None
}

fn snake_to_camel(s: &str) -> String {
    let mut out = String::new();
    let mut up = false;
    for ch in s.chars() {
        if ch == '_' {
            up = true;
        } else if up {
            out.push(ch.to_ascii_uppercase());
            up = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn camel_to_snake(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if ch.is_ascii_uppercase() {
            if !out.is_empty() {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// 基础 URL，从 `CMX_BASE_URL` 环境变量读取，默认 http://127.0.0.1:8080。
pub fn base_url() -> String {
    std::env::var("CMX_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
}

/// 开发环境静态 API Key（来自 `CMX_API_KEY` 环境变量，默认取 dev.toml 中的 dev key）。
///
/// 由于 dev 环境 `[auth].whitelist` 未实际放行 `/api/iam/**`，IAM 接口需鉴权。
/// 测试约定：当未提供 Bearer Token 时，统一附加 `X-API-Key` 头进行鉴权。
/// 需真实用户上下文的认证类接口（change_password/heartbeat 等）显式传入 Bearer Token。
pub fn api_key() -> String {
    std::env::var("CMX_API_KEY")
        .unwrap_or_else(|_| "cmx_sk_dev_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6".to_string())
}

/// 构造一个配置好超时的 reqwest 客户端。
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("构建 HTTP 客户端失败")
}

/// 生成带前缀的唯一字符串，如 `e2e_perm_a1b2c3d4`。
pub fn gen_id(prefix: &str) -> String {
    let id = uuid::Uuid::new_v4().simple().to_string();
    let short = &id[..8];
    format!("{prefix}_{short}")
}

/// 发送请求并解析为统一响应。
pub async fn send(_client: &reqwest::Client, req: reqwest::RequestBuilder) -> ApiResult {
    let resp = req
        .send()
        .await
        .unwrap_or_else(|e| panic!("请求失败: {e}"));
    let status = resp.status().as_u16();
    let text = resp.text().await.unwrap_or_default();
    let raw: Value = serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("响应非 JSON: {text} (解析错误: {e})");
    });
    let code = raw
        .get("code")
        .and_then(|v| v.as_u64())
        .map(|v| v as u16)
        // 若无 code 字段，回退用 HTTP 状态码
        .unwrap_or(status);
    let msg = raw
        .get("msg")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let data = raw.get("data").cloned();
    let pagination = raw.get("pagination").cloned();
    ApiResult {
        status,
        code,
        msg,
        data,
        pagination,
        raw,
    }
}

/// 发送 JSON POST 请求。
///
/// `token` 为 `None` 时附加开发 API Key 鉴权；为 `Some(t)` 时使用 Bearer Token。
pub async fn post_json(
    client: &reqwest::Client,
    path: &str,
    body: &Value,
    token: Option<&str>,
) -> ApiResult {
    let url = format!("{}{}", base_url(), path);
    let mut req = client.post(&url).json(body);
    req = apply_auth(req, token);
    send(client, req).await
}

/// 发送 GET 请求（带可选 query string 与 token）。
pub async fn get(
    client: &reqwest::Client,
    path: &str,
    query: Option<&[(&str, &str)]>,
    token: Option<&str>,
) -> ApiResult {
    let url = format!("{}{}", base_url(), path);
    let mut req = client.get(&url);
    if let Some(q) = query {
        req = req.query(q);
    }
    req = apply_auth(req, token);
    send(client, req).await
}

/// 应用鉴权：有 Bearer Token 用 Token，否则附加开发 API Key。
fn apply_auth(mut req: reqwest::RequestBuilder, token: Option<&str>) -> reqwest::RequestBuilder {
    if let Some(t) = token {
        req = req.bearer_auth(t);
    } else {
        req = req.header("X-API-Key", api_key());
    }
    req
}

/// 登录引导：创建唯一测试用户并登录，返回 access/refresh token 与用户名。
///
/// 依赖 dev 环境 `whitelist = ["/api/**"]`，`create_user` 无需认证即可调用。
#[allow(dead_code)]
pub struct TestUser {
    pub username: String,
    pub password: String,
    pub access_token: String,
    pub refresh_token: String,
}

#[allow(dead_code)]
pub async fn bootstrap_user() -> TestUser {
    let client = client();
    let username = gen_id("e2e_user");
    let password = format!("E2e@{}", gen_id("pw"));
    let body = serde_json::json!({
        "username": username,
        "password": password,
        "nickname": "E2E测试",
        "status": 1,
    });
    let res = post_json(&client, "/api/iam/users/create", &body, None).await;
    if res.code != 0 {
        // 可能用户已存在等，直接 panic 给出详情
        panic!("引导创建用户失败: code={} msg={}", res.code, res.msg);
    }

    // 登录
    let login_body = serde_json::json!({
        "username": username,
        "password": password,
    });
    let login = post_json(&client, "/api/auth/login", &login_body, None).await;
    let data = login.assert_success();
    let access_token = flex_get(&data, "access_token")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .expect("登录响应缺少 access_token");
    let refresh_token = flex_get(&data, "refresh_token")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .expect("登录响应缺少 refresh_token");

    TestUser {
        username,
        password,
        access_token,
        refresh_token,
    }
}

/// 健康检查：等待服务可达且 auth 健康。
#[allow(dead_code)]
pub async fn wait_for_server() {
    let client = client();
    let url = format!("{}/api/auth/health", base_url());
    for i in 0..60 {
        match client.get(&url).send().await {
            Ok(r) if r.status().is_success() => return,
            _ => {
                if i == 0 {
                    eprintln!("等待服务启动...");
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
    panic!("服务在 120s 内未就绪: {url}");
}
