fn main() {
    volo_build::ConfigBuilder::default()
        .write()
        .expect("volo-build 失败：请确认 idl/ 下 proto 与 volo.yml 配置正确");
}
