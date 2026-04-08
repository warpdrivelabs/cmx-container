DROP TABLE IF EXISTS cmx_domain;
CREATE TABLE cmx_domain(
                            id varchar(64) NOT NULL,
                           code varchar(64) NOT NULL,
                           name varchar(200) NOT NULL,
                           description text,
                           type varchar(50),
                           tags text,
                           sort_order int4 DEFAULT  0,
                           status int4 DEFAULT  1,
                           archived int4 DEFAULT  0,
                           create_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                           update_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                           create_by varchar(100),
                           create_name varchar(100),
                           update_by varchar(100),
                           update_name varchar(100),
                           PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_domain IS '域表';
COMMENT ON COLUMN cmx_domain.code IS '域编码，全局唯一，如: FIN, HR, SCM';
COMMENT ON COLUMN cmx_domain.name IS '域名称，如: 财务域, 人力资源域';
COMMENT ON COLUMN cmx_domain.description IS '域描述';
COMMENT ON COLUMN cmx_domain.type IS '类型: business(业务域), technical(技术域), product_line(产品线)';
COMMENT ON COLUMN cmx_domain.tags IS '多标签，JSON数组字符串，如 ["财务","核心","S4HANA"]';
COMMENT ON COLUMN cmx_domain.sort_order IS '排序字段，数值小的靠前';
COMMENT ON COLUMN cmx_domain.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_domain.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_domain.create_time IS '创建时间';
COMMENT ON COLUMN cmx_domain.update_time IS '更新时间';
COMMENT ON COLUMN cmx_domain.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_domain.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_domain.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_domain.update_name IS '更新人名称';
CREATE UNIQUE INDEX uk_cmx_domain_codee ON cmx_domain(code);



DROP TABLE IF EXISTS cmx_application;
CREATE TABLE cmx_application(
                                id varchar(64) NOT NULL,
                                code VARCHAR(64) NOT NULL,
                                domain_code VARCHAR(64) NOT NULL,
                                name VARCHAR(200) NOT NULL,
                                description TEXT,
                                type VARCHAR(50),
                                tags TEXT,
                                sort_order int4 DEFAULT  0,
                                status int4 DEFAULT  1,
                                archived int4 DEFAULT  0,
                                create_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                                update_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                                create_by VARCHAR(100),
                                create_name VARCHAR(100),
                                update_by VARCHAR(100),
                                update_name VARCHAR(100),
                                PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_application IS '应用表';
COMMENT ON COLUMN cmx_application.code IS '应用编码，全局唯一，如: FI, CO, MM';
COMMENT ON COLUMN cmx_application.domain_code IS '所属域编码，逻辑关联到cmx_domain.code';
COMMENT ON COLUMN cmx_application.name IS '应用名称，如: 财务会计, 管理会计';
COMMENT ON COLUMN cmx_application.description IS '应用描述';
COMMENT ON COLUMN cmx_application.type IS '类型: product(产品应用), platform(平台应用), integration(集成应用)';
COMMENT ON COLUMN cmx_application.tags IS '多标签，JSON数组字符串，如 ["财务核心","SAP_FI"]';
COMMENT ON COLUMN cmx_application.sort_order IS '排序字段，数值小的靠前';
COMMENT ON COLUMN cmx_application.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_application.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_application.create_time IS '创建时间';
COMMENT ON COLUMN cmx_application.update_time IS '更新时间';
COMMENT ON COLUMN cmx_application.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_application.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_application.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_application.update_name IS '更新人名称';
CREATE UNIQUE INDEX uk_cmx_application_codee ON cmx_application(code);


DROP TABLE IF EXISTS cmx_module;
CREATE TABLE cmx_module(
                           id varchar(64) NOT NULL,
                           code VARCHAR(64) NOT NULL,
                           domain_code VARCHAR(64) NOT NULL,
                           application_code VARCHAR(64) NOT NULL,
                           name VARCHAR(200) NOT NULL,
                           description TEXT,
                           type VARCHAR(50),
                           tags TEXT,
                           sort_order int4 DEFAULT  0,
                           status int4 DEFAULT  1,
                           archived int4 DEFAULT  0,
                           create_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                           update_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                           create_by VARCHAR(100),
                           create_name VARCHAR(100),
                           update_by VARCHAR(100),
                           update_name VARCHAR(100),
                           PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_module IS '模块表';
COMMENT ON COLUMN cmx_module.code IS '模块编码，全局唯一，如: GL, AR, AP';
COMMENT ON COLUMN cmx_module.domain_code IS '所属域编码';
COMMENT ON COLUMN cmx_module.application_code IS '所属应用编码，逻辑关联到cmx_application.code';
COMMENT ON COLUMN cmx_module.name IS '模块名称，如: 总账模块, 应收模块';
COMMENT ON COLUMN cmx_module.description IS '模块描述';
COMMENT ON COLUMN cmx_module.type IS '类型: business(业务模块), extension(扩展点), integration(集成点)';
COMMENT ON COLUMN cmx_module.tags IS '多标签，JSON数组字符串，如 ["总账","核心","FI-GL"]';
COMMENT ON COLUMN cmx_module.sort_order IS '排序字段，数值小的靠前';
COMMENT ON COLUMN cmx_module.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_module.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_module.create_time IS '创建时间';
COMMENT ON COLUMN cmx_module.update_time IS '更新时间';
COMMENT ON COLUMN cmx_module.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_module.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_module.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_module.update_name IS '更新人名称';

CREATE UNIQUE INDEX uk_cmx_module_codee ON cmx_module(code);





DROP TABLE IF EXISTS cmx_sys_datasource;
CREATE TABLE cmx_sys_datasource(
                                   id VARCHAR(64) NOT NULL,
                                   db_id VARCHAR(64),
                                   db_schema VARCHAR(64),
                                   description VARCHAR(255),
                                   db_type VARCHAR(255) NOT NULL,
                                   db_url VARCHAR(255),
                                   max_connections INTEGER,
                                   min_connections INTEGER,
                                   connect_timeout INTEGER,
                                   idle_timeout INTEGER,
                                   max_lifetime INTEGER,
                                   health_check_interval INTEGER,
                                   health_check_timeout INTEGER,
                                   default_flag INTEGER,
                                   status int4 DEFAULT  1,
                                   archived int4 DEFAULT  0,
                                   create_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                                   update_time timestamp DEFAULT  CURRENT_TIMESTAMP,
                                   create_by varchar(100),
                                   create_name varchar(100),
                                   update_by varchar(100),
                                   update_name varchar(100),
                                   PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_sys_datasource IS 'cmx数据源管理';
COMMENT ON COLUMN cmx_sys_datasource.id IS '主键';
COMMENT ON COLUMN cmx_sys_datasource.db_id IS '数据源标识';
COMMENT ON COLUMN cmx_sys_datasource.db_schema IS '数据库模式';
COMMENT ON COLUMN cmx_sys_datasource.description IS '数据源描述';
COMMENT ON COLUMN cmx_sys_datasource.db_type IS '数据库类型(postgres;mysql)';
COMMENT ON COLUMN cmx_sys_datasource.db_url IS '数据库连接 URL';
COMMENT ON COLUMN cmx_sys_datasource.max_connections IS '最大连接数';
COMMENT ON COLUMN cmx_sys_datasource.min_connections IS '最小空闲连接数';
COMMENT ON COLUMN cmx_sys_datasource.connect_timeout IS '连接超时时间（秒）';
COMMENT ON COLUMN cmx_sys_datasource.idle_timeout IS '空闲连接超时时间（秒）';
COMMENT ON COLUMN cmx_sys_datasource.max_lifetime IS '最大生命周期（秒）';
COMMENT ON COLUMN cmx_sys_datasource.health_check_interval IS '健康检查间隔（秒）';
COMMENT ON COLUMN cmx_sys_datasource.health_check_timeout IS '健康检查超时（秒）';
COMMENT ON COLUMN cmx_sys_datasource.default_flag IS '是否默认;0否1是';
COMMENT ON COLUMN cmx_sys_datasource.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_sys_datasource.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_sys_datasource.create_time IS '创建时间';
COMMENT ON COLUMN cmx_sys_datasource.update_time IS '更新时间';
COMMENT ON COLUMN cmx_sys_datasource.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_sys_datasource.create_name IS '创建人名称';
COMMENT ON COLUMN cmx_sys_datasource.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_sys_datasource.update_name IS '更新人名称';
CREATE UNIQUE INDEX uk_cmx_datasource_db_id ON cmx_sys_datasource(db_id);

