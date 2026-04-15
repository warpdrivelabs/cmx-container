DROP TABLE IF EXISTS cmx_service_define;
CREATE TABLE cmx_service_define(
                                   id VARCHAR(64) NOT NULL,
                                   service_key VARCHAR(100) NOT NULL,
                                   service_name VARCHAR(100),
                                   description VARCHAR(255),
                                   plugin_id VARCHAR(64),
                                   domain_code           VARCHAR(64),
                                   application_code      VARCHAR(64),
                                   module_code           VARCHAR(64),
                                   status int4 DEFAULT  1,
                                   version VARCHAR(50),
                                   create_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                                   update_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                                   create_by VARCHAR(100),
                                   create_name VARCHAR(100),
                                   update_by VARCHAR(100),
                                   update_name VARCHAR(100),
                                   archived int4 DEFAULT  0,
                                   PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_service_define IS '服务定义表';
COMMENT ON COLUMN cmx_service_define.id IS '主键';
COMMENT ON COLUMN cmx_service_define.service_key IS '服务key';
COMMENT ON COLUMN cmx_service_define.service_name IS '服务名称';
COMMENT ON COLUMN cmx_service_define.description IS '服务描述';
COMMENT ON COLUMN cmx_service_define.plugin_id IS '所属插件id';
COMMENT ON COLUMN cmx_service_define.domain_code IS '所属域';
COMMENT ON COLUMN cmx_service_define.application_code IS '所属应用';
COMMENT ON COLUMN cmx_service_define.module_code IS '所属模块';
COMMENT ON COLUMN cmx_service_define.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_service_define.version IS '服务版本';

COMMENT ON COLUMN cmx_service_define.create_time IS '创建时间';
COMMENT ON COLUMN cmx_service_define.update_time IS '更新时间';
COMMENT ON COLUMN cmx_service_define.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_service_define.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_service_define.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_service_define.update_name IS '更新人名称';
COMMENT ON COLUMN cmx_service_define.archived IS '归档标志：0-未归档，1-已归档';

CREATE UNIQUE INDEX uk_cmx_service_define_key ON cmx_service_define(service_key);

DROP TABLE IF EXISTS cmx_service_define_version;
CREATE TABLE cmx_service_define_version(
                                           id VARCHAR(64) NOT NULL,
                                           service_key VARCHAR(100),
                                           version VARCHAR(50),
                                           plugin_id VARCHAR(64),
                                           plugin_version VARCHAR(50),
                                           config TEXT,
                                           create_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                                           update_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                                           create_by VARCHAR(100),
                                           create_name VARCHAR(100),
                                           update_by VARCHAR(100),
                                           update_name VARCHAR(100),
                                           archived int4 DEFAULT  0,
                                           PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_service_define_version IS '服务定义版本表';
COMMENT ON COLUMN cmx_service_define_version.id IS '主键';
COMMENT ON COLUMN cmx_service_define_version.service_key IS '服务key';
COMMENT ON COLUMN cmx_service_define_version.version IS '服务版本';
COMMENT ON COLUMN cmx_service_define_version.plugin_id IS '服务所属插件';
COMMENT ON COLUMN cmx_service_define_version.plugin_version IS '所属插件版本';
COMMENT ON COLUMN cmx_service_define_version.config IS '服务编排配置';
COMMENT ON COLUMN cmx_service_define_version.create_time IS '创建时间';
COMMENT ON COLUMN cmx_service_define_version.update_time IS '更新时间';
COMMENT ON COLUMN cmx_service_define_version.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_service_define_version.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_service_define_version.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_service_define_version.update_name IS '更新人名称';
COMMENT ON COLUMN cmx_service_define_version.archived IS '归档标志：0-未归档，1-已归档';
create index cmx_service_define_version_service_key_index
    on cmx_service_define_version (service_key);

