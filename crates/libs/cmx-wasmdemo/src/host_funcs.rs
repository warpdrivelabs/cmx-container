//! WASM 宿主函数导入声明
//!
//! 声明所有从宿主环境导入的函数，供 WASM 模块调用。
//! 所有函数签名: (input_ptr: i32, input_len: i32) -> i32
//!
//! # 数据传递协议
//!
//! - 输入: 通过 (input_ptr, input_len) 指向 WASM 线性内存中的数据
//! - 输出: 返回值是输出数据长度（负值表示错误）
//! - 实际输出数据通过 get_output 函数读取
//! - 格式: 大多数函数使用 JSON 格式

// cmx:log 命名空间 - 日志记录函数
#[link(wasm_import_module = "cmx:log")]
extern "C" {
    /// 记录 info 级别日志
    pub fn info(input_ptr: i32, input_len: i32) -> i32;
    /// 记录 warn 级别日志
    pub fn warn(input_ptr: i32, input_len: i32) -> i32;
    /// 记录 error 级别日志
    pub fn error(input_ptr: i32, input_len: i32) -> i32;
}

// cmx:database 命名空间 - 数据库操作函数
#[link(wasm_import_module = "cmx:database")]
extern "C" {
    /// 执行写操作 SQL
    pub fn execute_sql(input_ptr: i32, input_len: i32) -> i32;
    /// 执行查询 SQL
    pub fn query_sql(input_ptr: i32, input_len: i32) -> i32;
    /// 开启事务
    pub fn txn_begin(input_ptr: i32, input_len: i32) -> i32;
    /// 提交事务
    pub fn txn_commit(input_ptr: i32, input_len: i32) -> i32;
    /// 回滚事务
    pub fn txn_rollback(input_ptr: i32, input_len: i32) -> i32;
}

// cmx:buffer 命名空间 - 缓存操作函数
#[link(wasm_import_module = "cmx:buffer")]
extern "C" {
    /// 读取缓存
    pub fn cache_get(input_ptr: i32, input_len: i32) -> i32;
    /// 写入缓存
    pub fn cache_set(input_ptr: i32, input_len: i32) -> i32;
    /// 删除缓存
    pub fn cache_delete(input_ptr: i32, input_len: i32) -> i32;
}

// cmx:plugin 命名空间 - 插件间调用函数
#[link(wasm_import_module = "cmx:plugin")]
extern "C" {
    /// 调用其他插件的服务
    pub fn call_service(input_ptr: i32, input_len: i32) -> i32;
    /// 获取当前插件信息
    pub fn get_info(input_ptr: i32, input_len: i32) -> i32;
}
