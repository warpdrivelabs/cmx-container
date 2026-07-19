//! Form Filter 反序列化单元测试(无 DB)
use cmx_biz::form::FormFilter;

#[test]
fn test_form_filter_deserialize_basic() {
    // 反序列化一个简单的 code 等值过滤
    let json = r#"{"code": {"$eq": "gl:voucher_form"}}"#;
    let filter: FormFilter = serde_json::from_str(json).expect("反序列化应成功");
    assert!(filter.code.is_some(), "code 字段应被解析");
    assert!(filter.name.is_none(), "name 未提供应为 None");
}

#[test]
fn test_form_filter_default_is_all_none() {
    let filter = FormFilter::default();
    assert!(filter.code.is_none());
    assert!(filter.module_code.is_none());
    assert!(filter.status.is_none());
    assert!(filter.archived.is_none());
}

#[test]
fn test_form_filter_deserialize_multi_fields() {
    let json = r#"{
        "module_code": {"$eq": "GL"},
        "name": {"$contains": "表单"},
        "status": {"$eq": 1}
    }"#;
    let filter: FormFilter = serde_json::from_str(json).expect("多字段反序列化应成功");
    assert!(filter.module_code.is_some());
    assert!(filter.name.is_some());
    assert!(filter.status.is_some());
}
