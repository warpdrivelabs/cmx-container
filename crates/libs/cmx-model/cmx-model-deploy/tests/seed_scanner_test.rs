use cmx_model_deploy::seed_scanner::{aggregate_sha256, scan_menu_files_in_dir, scan_seed_files_in_dir, ScannedFile};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_scan_seed_files_reads_json_arrays() {
    let dir = tempdir().unwrap();
    let seed_dir = dir.path().join("seed");
    fs::create_dir_all(&seed_dir).unwrap();
    
    fs::write(
        seed_dir.join("cf_dc_indicator.json"),
        r#"[
            {"code":"S","name":"借方","sort_no":1},
            {"code":"H","name":"贷方","sort_no":2}
        ]"#,
    ).unwrap();
    fs::write(
        seed_dir.join("cf_account_type.json"),
        r#"[{"code":"A","name":"资产类"}]"#,
    ).unwrap();
    // 非 json 应被忽略
    fs::write(seed_dir.join("README.md"), "# doc").unwrap();
    
    let files = scan_seed_files_in_dir(&seed_dir);
    assert_eq!(files.len(), 2);
    
    let indicator = files.iter().find(|f| f.table_name == "cf_dc_indicator").unwrap();
    assert_eq!(indicator.row_count, 2);
    assert_eq!(indicator.checksum.len(), 64); // SHA256 hex
    
    let account = files.iter().find(|f| f.table_name == "cf_account_type").unwrap();
    assert_eq!(account.row_count, 1);
}

#[test]
fn test_scan_menu_files_counts_nodes_recursively() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("explorer-menu.json"),
        r#"{
            "version": 1,
            "items": [
                {"id":"root","caption":"根","children":[
                    {"id":"c1","caption":"子1"},
                    {"id":"c2","caption":"子2","children":[
                        {"id":"g1","caption":"孙1"}
                    ]}
                ]},
                {"id":"root2","caption":"根2"}
            ]
        }"#,
    ).unwrap();
    
    let files = scan_menu_files_in_dir(dir.path());
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].row_count, 5); // root + c1 + c2 + g1 + root2
    assert_eq!(files[0].table_name, ""); // MENU 不用 table_name
}

#[test]
fn test_aggregate_sha256_is_order_independent() {
    let mk = |path: &str, content: &str| ScannedFile {
        table_name: String::new(),
        rel_path: path.to_string(),
        content: content.to_string(),
        checksum: String::new(),
        row_count: 0,
        modified_date: None,
    };
    
    let set1 = vec![
        mk("a.json", "content_a"),
        mk("b.json", "content_b"),
    ];
    let set2 = vec![
        mk("b.json", "content_b"),  // 顺序反了
        mk("a.json", "content_a"),
    ];
    
    assert_eq!(aggregate_sha256(&set1), aggregate_sha256(&set2));
}

#[test]
fn test_aggregate_sha256_changes_when_content_changes() {
    let mk = |content: &str| ScannedFile {
        table_name: String::new(),
        rel_path: "a.json".to_string(),
        content: content.to_string(),
        checksum: String::new(),
        row_count: 0,
        modified_date: None,
    };
    
    assert_ne!(aggregate_sha256(&[mk("v1")]), aggregate_sha256(&[mk("v2")]));
}
