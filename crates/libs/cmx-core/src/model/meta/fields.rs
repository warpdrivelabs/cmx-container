use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, Display, EnumString};

// 宏用于自动实现字段enum的通用方法
macro_rules! impl_field_enum {
    ($enum_name:ident) => {
        impl $enum_name {
            pub fn from_str(s: &str) -> Option<Self> {
                s.parse().ok()
            }

            pub fn to_str(&self) -> &str {
                self.as_ref()
            }
        }
    };
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize, EnumString, AsRefStr)]
pub enum SYS_MODEL {
    #[strum(serialize = "MDL_ID")]
    MDL_ID,             // 模型ID
    #[strum(serialize = "MDL_MC")]
    MDL_MC,             // 模型名称
    #[strum(serialize = "MDL_TYPE")]
    MDL_TYPE,           // 模型类型
    #[strum(serialize = "MDL_KEYDCT")]
    MDL_KEYDCT,         // 键字典
    #[strum(serialize = "MDL_UNITDCT")]
    MDL_UNITDCT,        // 单位字典
    #[strum(serialize = "KEY_SET")]
    KEY_SET,            // 键设置
    #[strum(serialize = "SYS_ID")]
    SYS_ID,             // 系统ID
    #[strum(serialize = "F_STAU")]
    F_STAU,             // 状态
    #[strum(serialize = "F_CRDATE")]
    F_CRDATE,           // 创建时间
    #[strum(serialize = "F_CHDATE")]
    F_CHDATE,           // 修改时间
    #[strum(serialize = "MDL_SJLX")]
    MDL_SJLX,           // 数据类型
    #[strum(serialize = "MDL_NDCT")]
    MDL_NDCT,           // 年度字典
    #[strum(serialize = "MDL_YDCT")]
    MDL_YDCT,           // 月度字典
    #[strum(serialize = "MDL_RDCT")]
    MDL_RDCT,           // 日字典
    #[strum(serialize = "MDL_BHDCT")]
    MDL_BHDCT,          // 编号字典
    #[strum(serialize = "MDL_TYDCT")]
    MDL_TYDCT,          // 统一字典
    #[strum(serialize = "CVT_UNITSET")]
    CVT_UNITSET,        // 单位设置转换
    #[strum(serialize = "F_BM_STAU")]
    F_BM_STAU,          // 编码状态
    #[strum(serialize = "F_DF_STAU")]
    F_DF_STAU,          // 默认状态
    #[strum(serialize = "F_CRUSER")]
    F_CRUSER,           // 创建人
    #[strum(serialize = "F_CHUSER")]
    F_CHUSER,           // 修改人
    #[strum(serialize = "F_ENABLE")]
    F_ENABLE,           // 是否启用
    #[strum(serialize = "F_MODELFKEY")]
    F_MODELFKEY,        // 模型外键
    #[strum(serialize = "OBJ_GROUP")]
    OBJ_GROUP,          // 对象组
}

impl_field_enum!(SYS_MODEL);

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize,EnumString, Display,AsRefStr)]
pub enum SYS_MDL_CTN {
    MDL_ID,             // 模型ID
    CTN_ID,             // 内容ID
    CTN_TYPE,           // 内容类型
    CTN_MC,             // 内容名称
    CTN_FCT1,           // 内容要素1
    CTN_FCT2,           // 内容要素2
    CTN_FCT3,           // 内容要素3
    CTN_FCT4,           // 内容要素4
    CTN_FCT5,           // 内容要素5
    CTN_FCT6,           // 内容要素6
    CTN_FCT7,           // 内容要素7
    CTN_FCT8,           // 内容要素8
    CTN_FCT9,           // 内容要素9
    CTN_FCT10,          // 内容要素10
    CTN_FCT11,          // 内容要素11
    CTN_FCT12,          // 内容要素12
    CTN_FCT13,          // 内容要素13
    CTN_FCT14,          // 内容要素14
    CTN_FCT15,          // 内容要素15
    CTN_FCT16,          // 内容要素16
    PCTN_ID,            // 父内容ID
    F_CRDATE,           // 创建时间
    F_CHDATE,           // 修改时间
    F_CRUSER,           // 创建人
    F_CHUSER,           // 修改人
}

impl SYS_MDL_CTN {
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize,EnumString, Display,AsRefStr)]
pub enum SYS_MDL_VAL {
    MDL_ID,             // 模型ID
    MDL_KEY,            // 模型键
    UNIT_ID,            // 单位ID
    MDL_VALUE,          // 模型值
    MDL_NOTE,           // 模型备注
}

impl SYS_MDL_VAL {
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize,EnumString, Display,AsRefStr)]
pub enum SYS_RLGL {
    RLGL_ID,            // 关系ID
    RLGL_MC,            // 关系名称
    DCT_ID1,            // 字典ID1
    DCT_ID2,            // 字典ID2
    RLGL_OBJID,         // 关系对象ID
    DCT_BMCOL1,         // 字典编码列1
    DCT_BMCOL2,         // 字典编码列2
    RLGL_YEAR,          // 年度标识
    RLGL_UNIT,          // 单位标识
    RLGL_USCST,         // 使用自定义
    RLGL_TYPE,          // 关系类型
    SYS_ID,             // 系统ID
    ISCREATEDDW,        // 是否创建数据仓库
    ISSORTYEAR,         // 是否按年排序
    ISWH,               // 是否维护
    ISORDER,            // 是否排序
    ISMUSTMX_SUBDCT,    // 是否必须明细子字典
    ISCHECK_SAMEGRADE,  // 是否检查同级
    RLT_OBJID,          // 关联对象ID
    F_CRDATE,           // 创建时间
    F_CHDATE,           // 修改时间
    F_CRUSER,           // 创建人
    F_CHUSER,           // 修改人
}

impl SYS_RLGL {
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize,EnumString, Display,AsRefStr)]
pub enum SYS_DICTS {
    DCT_ID,             // 字典ID
    OBJ_ID,             // 对象ID
    DCT_MC,             // 字典名称
    DCT_NAME,           // 字典名
    DCT_DES,            // 字典描述
    DCT_QXOBJID,        // 权限对象ID
    DCT_TYPE,           // 字典类型
    DCT_BMCOLID,        // 编码列ID
    DCT_BZCOLID,        // 标准列ID
    DCT_MCCOLID,        // 名称列ID
    DCT_JSCOLID,        // 级数列ID
    DCT_MXCOLID,        // 明细列ID
    DCT_BMSTRU,         // 编码结构
    DCT_PTCOLID,        // 平台列ID
    DCT_KZCOLID,        // 扩展列ID
    SYS_ID,             // 系统ID
    DCT_FIXLEN,         // 固定长度
    DCT_STEP,           // 步长
    DCT_KPI,            // KPI标识
    DCT_SELECT,         // 选择标识
    DCT_MUNIT,          // 多单位标识
    DCT_CTLONE,         // 控制一个
    DCT_EXTAUTO,        // 扩展自动
    DCT_CREATE,         // 创建标识
    DCT_CHANGE,         // 变更标识
    DCT_NOTUSE,         // 不使用标识
    DCT_SUBJECT,        // 主题
    DCT_UNIT,           // 单位
    DCT_CST,            // 自定义
    DCT_FKEY1,          // 外键1
    DCT_FKEYDCT1,       // 外键字典1
    DCT_FKEY2,          // 外键2
    DCT_FKEYDCT2,       // 外键字典2
    DCT_FKEY3,          // 外键3
    DCT_FKEYDCT3,       // 外键字典3
    DCT_FKEY4,          // 外键4
    DCT_FKEYDCT4,       // 外键字典4
    DCT_FKEY5,          // 外键5
    DCT_FKEYDCT5,       // 外键字典5
    DCT_FKEY6,          // 外键6
    DCT_FKEYDCT6,       // 外键字典6
    DCT_FKEY7,          // 外键7
    DCT_FKEYDCT7,       // 外键字典7
    DCT_FKEY8,          // 外键8
    DCT_FKEYDCT8,       // 外键字典8
    DCT_SYNCDATE,       // 同步时间
    F_STAU,             // 状态
    F_CRDATE,           // 创建时间
    F_CHDATE,           // 修改时间
    DCT_QXSTAT,         // 权限状态
    DCT_AFFIXDCT,       // 附加字典
    F_GUID,             // 全局唯一标识
    F_CRUSER,           // 创建人
    F_CHUSER,           // 修改人
    F_ENABLE,           // 是否启用
    F_DELETED,          // 是否删除
    F_DICTSFKEY,        // 字典外键
}

impl SYS_DICTS {
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize,EnumString, Display,AsRefStr)]
pub enum SYS_DCT_CST {
    DCT_ID,             // 字典ID
    UNIT_ID,            // 单位ID
    DCT_KEY,            // 字典键
    DCT_VALUE,          // 字典值
    F_NOTE,             // 备注
    F_CRDATE,           // 创建时间
    F_CHDATE,           // 修改时间
    F_CRUSER,           // 创建人
    F_CHUSER,           // 修改人
}

impl SYS_DCT_CST {
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize,EnumString, Display,AsRefStr)]
pub enum SYS_KEYS {
    KEY_ID,             // 键ID
    OBJ_ID,             // 对象ID
    KEY_TYPE,           // 键类型
    OBJ_DEPID,          // 对象依赖ID
    KEY_CNT,            // 键数量
    KEY_PINDEX1,        // 主索引1
    KEY_PINDEX2,        // 主索引2
    KEY_PINDEX3,        // 主索引3
    KEY_PINDEX4,        // 主索引4
    KEY_PINDEX5,        // 主索引5
    KEY_PINDEX6,        // 主索引6
    KEY_PINDEX7,        // 主索引7
    KEY_PINDEX8,        // 主索引8
    KEY_PINDEX9,        // 主索引9
    KEY_PINDEX10,       // 主索引10
    KEY_PINDEX11,       // 主索引11
    KEY_PINDEX12,       // 主索引12
    KEY_PINDEX13,       // 主索引13
    KEY_PINDEX14,       // 主索引14
    KEY_PINDEX15,       // 主索引15
    KEY_PINDEX16,       // 主索引16
    KEY_FINDEX1,        // 外键索引1
    KEY_FINDEX2,        // 外键索引2
    KEY_FINDEX3,        // 外键索引3
    KEY_FINDEX4,        // 外键索引4
    KEY_FINDEX5,        // 外键索引5
    KEY_FINDEX6,        // 外键索引6
    KEY_FINDEX7,        // 外键索引7
    KEY_FINDEX8,        // 外键索引8
    F_STAU,             // 状态
    F_CRDATE,           // 创建时间
    F_CHDATE,           // 修改时间
    KEY_INDEX,          // 键索引
}

impl SYS_KEYS {
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize,EnumString, Display,AsRefStr)]
pub enum SYS_INDEXS {
    INX_ID,             // 索引ID
    INX_NAME,           // 索引名称
    INX_TYPE,           // 索引类型
    INX_CLT,            // 聚集索引
    OBJ_ID,             // 对象ID
    INX_COLS,           // 索引列
    F_STAU,             // 状态
    F_CRDATE,           // 创建时间
    F_CHDATE,           // 修改时间
    F_CRUSER,           // 创建人
    F_CHUSER,           // 修改人
}

impl SYS_INDEXS {
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize,EnumString, Display,AsRefStr)]
pub enum SYS_OBJCOLS {
    OBJ_ID,             // 对象ID
    COL_BASE,           // 基础列
    COL_ID,             // 列ID
    COL_MC,             // 列名称
    COL_DES,            // 列描述
    COL_TYPE,           // 列类型
    COL_APPTYPE1,       // 应用类型1
    COL_APPTYPE2,       // 应用类型2
    COL_CSTATT,         // 自定义属性
    COL_LEN,            // 长度
    COL_PREC,           // 精度
    COL_SCALE,          // 小数位
    COL_ISKEY,          // 是否主键
    COL_ISNULL,         // 是否可空
    COL_VISIBLE,        // 是否可见
    COL_EDITABLE,       // 是否可编辑
    COL_EDIT,           // 编辑类型
    COL_VIEW,           // 视图定义
    COL_DEFAULT,        // 默认值
    COL_DISP,           // 显示顺序
    COL_ORDER,          // 排序标识
    COL_MUNIT,          // 多单位标识
    COL_USE,            // 使用标识
    COL_VALUE,          // 值
    COL_ISFKEY,         // 是否外键
    COL_FOBJ,           // 外键对象
    COL_LANG,           // 语言标识
    COL_ISCALC,         // 是否计算列
    COL_COLGS,          // 列公式
    COL_CHECK,          // 检查约束
    COL_REGEXREF,       // 正则引用
    COL_ALIAS,          // 别名
    COL_COLJY,          // 列校验
    COL_CONT,           // 内容类型
    COL_EXT_ID,         // 扩展ID
    COL_OPT,            // 选项
    COL_REF,            // 引用
    F_LANG,             // 语言
    F_GUID,             // 全局唯一标识
    F_STAU,             // 状态
    F_CRDATE,           // 创建时间
    F_CHDATE,           // 修改时间
    COL_FKEY,           // 外键
    SYS_ID,             // 系统ID
    COL_ADD_LOG,        // 日志添加
    F_CRUSER,           // 创建人
    F_CHUSER,           // 修改人
}

impl SYS_OBJCOLS {
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize,EnumString, Display,AsRefStr)]
pub enum SYS_OBJECTS {
    OBJ_ID,             // 对象ID
    OBJ_MC,             // 对象名称
    OBJ_NAME,           // 对象名
    OBJ_DES,            // 对象描述
    OBJ_TYPE,           // 对象类型
    OBJ_APPTYPE,        // 应用类型
    OBJ_BUILDNO,        // 构建号
    SYS_ID,             // 系统ID
    OBJ_PKEYS,          // 主键
    OBJ_LANG,           // 语言标识
    OBJ_MUNIT,          // 多单位标识
    OBJ_CSTCOL,         // 自定义列
    OBJ_UTRG,           // 更新触发器
    OBJ_ITRG,           // 插入触发器
    OBJ_DTRG,           // 删除触发器
    OBJ_STRG,           // 状态触发器
    OBJ_TEMP,           // 临时表
    OBJ_MLID,           // 多语言ID
    F_GUID,             // 全局唯一标识
    OBJ_REF,            // 对象引用
    F_STAU,             // 状态
    F_CRDATE,           // 创建时间
    F_CHDATE,           // 修改时间
    OBJ_UNIT,           // 对象单位
    UNIT_ID,            // 单位ID
    TABLE_SPACE,        // 表空间
    INDEX_SPACE,        // 索引空间
    F_CRUSER,           // 创建人
    F_CHUSER,           // 修改人
    OBJ_LANGINIT,       // 语言初始化
    F_ENABLE,           // 是否启用
    OBJ_GROUP,          // 对象组
    OBJ_TEMPTYPE,       // 临时表类型
}

impl SYS_OBJECTS {
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize,EnumString, Display,AsRefStr)]
pub enum SYS_OBJ_VAL {
    OBJ_ID,             // 对象ID
    OBJ_KEY,            // 对象键
    OBJ_VALUE,          // 对象值
    F_NOTE,             // 备注
}

impl SYS_OBJ_VAL {
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize,EnumString, Display,AsRefStr)]

pub enum SYS_FACTS {
    FCT_ID,             // 事实ID
    FCT_MC,             // 事实名称
    FCT_NAME,           // 事实名
    FCT_DES,            // 事实描述
    FCT_TYPE,           // 事实类型
    FCT_TMTYPE,         // 时间类型
    OBJ_ID,             // 对象ID
    BILL_BH_COL,        // 单据编号列
    BLFL_BH_COL,        // 分类编号列
    BLMX_BH_COL,        // 明细编号列
    GRP_ID1,            // 分组ID1
    GRP_ID2,            // 分组ID2
    GRP_ID3,            // 分组ID3
    GRP_ID4,            // 分组ID4
    GRP_ID5,            // 分组ID5
    GRP_ID6,            // 分组ID6
    GRP_ID7,            // 分组ID7
    GRP_ID8,            // 分组ID8
    GRP_ID9,            // 分组ID9
    GRP_ID10,           // 分组ID10
    GRP_ID11,           // 分组ID11
    GRP_ID12,           // 分组ID12
    GRP_ID13,           // 分组ID13
    GRP_ID14,           // 分组ID14
    GRP_ID15,           // 分组ID15
    GRP_ID16,           // 分组ID16
    SYS_ID,             // 系统ID
    F_STAU,             // 状态
    F_CRDATE,           // 创建时间
    F_CHDATE,           // 修改时间
}

impl SYS_FACTS {
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize,EnumString, Display,AsRefStr)]
pub enum SYS_OPLOG {
    LOG_ID,             // 日志ID
    F_GNBH,             // 功能编号
    F_GNMC,             // 功能名称
    F_SJ,               // 事件
    F_STIME,            // 开始时间
    F_ETIME,            // 结束时间
    F_USER,             // 用户
    F_NAME,             // 名称
    F_CLIENT,           // 客户端
    F_IP,               // IP地址
    F_STR01,            // 字符串1
    F_STR03,            // 字符串3
    F_STR04,            // 字符串4
    F_STR05,            // 字符串5
    F_STR02,            // 字符串2
    F_PRODUCT,          // 产品
    F_XTBH,             // 系统编号
    F_GNMK,             // 功能模块
    F_FORMS,            // 表单
    F_PREPAREVIEW,      // 预览
    F_SERVERID,         // 服务器ID
    F_NODEID,           // 节点ID
    F_CONTENTVIEW,      // 内容视图
    F_MAC,              // MAC地址
    F_URL,              // URL
    F_TYPE,             // 类型
    F_REQUEST,          // 请求
    F_RESPONSE,         // 响应
}

impl SYS_OPLOG {
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize,EnumString, Display,AsRefStr)]
pub enum BSCONF {
    F_VKEY,             // 键
    F_VAL,              // 值
    F_NOTE,             // 备注
    F_TYPE,             // 类型
    UNIT_ID,            // 单位ID
    F_SYS,              // 系统
}

impl BSCONF {
    pub fn from_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    pub fn to_str(&self) -> &str {
        self.as_ref()
    }
}