fn main() {
    volo_build::ConfigBuilder::default()
        .write()
        .expect("volo-build 失败：请确认 idl/cmx_service.proto 与 volo.yml 配置正确");
}
