alter table cmx_plugin_versions
    add build_type varchar(30);

comment on column cmx_plugin_versions.build_type is '构建类型: debug/release';





alter table cmx_meta_table_define
    add display_name varchar(100);

comment on column cmx_meta_table_define.display_name is '显示名称';


alter table cmx_meta_table_define_version
    add display_name varchar(100);

comment on column cmx_meta_table_define_version.display_name is '显示名称';
