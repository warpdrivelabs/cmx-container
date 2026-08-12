//! msgpack 成功信封编码（doc/dct 列式二进制响应共用）。
//!
//! 消除 doc-api / dct-api 各自复刻的 `encode_envelope_ok`。

use axum::http::header;
use axum::response::{IntoResponse, Response};

/// 成功信封的 msgpack 字节：`{code:0, msg:"success", data:<列式包字节>}`。
///
/// `rmp::encode` 的写入方法只在 buf 写入失败时返回 Err（Vec 写入不会失败），
/// 故用 expect 表达「固定结构写入不可能失败」的断言。
pub fn encode_envelope_ok(data_msgpack: &[u8]) -> Vec<u8> {
    use rmp::encode as mp;
    let mut buf = Vec::with_capacity(data_msgpack.len() + 32);
    mp::write_map_len(&mut buf, 3).expect("msgpack 写 map_len 不应失败");
    mp::write_str(&mut buf, "code").expect("msgpack 写 str 不应失败");
    mp::write_uint(&mut buf, 0).expect("msgpack 写 uint 不应失败");
    mp::write_str(&mut buf, "msg").expect("msgpack 写 str 不应失败");
    mp::write_str(&mut buf, "success").expect("msgpack 写 str 不应失败");
    mp::write_str(&mut buf, "data").expect("msgpack 写 str 不应失败");
    buf.extend_from_slice(data_msgpack);
    buf
}

/// 把列式包字节包成 `application/x-msgpack` 响应（成功信封）。
///
/// doc/dct 的 zmc msgpack 端点共用：`列式包 -> encode_envelope_ok -> Response`。
pub fn msgpack_ok_response(columnar: &[u8]) -> Response {
    let body = encode_envelope_ok(columnar);
    (
        [(header::CONTENT_TYPE, "application/x-msgpack")],
        body,
    )
        .into_response()
}
