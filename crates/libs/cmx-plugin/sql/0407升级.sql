alter table cmx_plugin_versions
    add build_type varchar(30);

comment on column cmx_plugin_versions.build_type is '构建类型: debug/release';
