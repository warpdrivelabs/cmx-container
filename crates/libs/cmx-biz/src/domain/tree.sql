WITH domain_tree AS (
    -- 第一级：域
    SELECT
        id as id,
        NULL as parent_code,
        d.code as code,
        d.name as name,
        d.description as description,
        d.type as type,
        d.tags as tags,
        d.icon as icon,
        d.title as title,
        d.sort_order as sort_order,
        d.status as status,
        d.archived as archived,
        d.create_time as create_time,
        d.update_time as update_time,
        d.create_by as create_by,
        d.create_name as create_name,
        d.update_by as update_by,
        d.update_name as update_name,
        'domain' as node_type,
        1 as level,
        d.code as domain_code,
        NULL as application_code,
        NULL as module_code
    FROM public.cmx_domain d
    WHERE d.status = 1 AND d.archived = 0

    UNION ALL

    -- 第二级：应用
    SELECT
        id as id,
        a.domain_code as parent_code,
        a.code as code,
        a.name as name,
        a.description as description,
        a.type as type,
        a.tags as tags,
        a.icon as icon,
        a.title as title,
        a.sort_order as sort_order,
        a.status as status,
        a.archived as archived,
        a.create_time as create_time,
        a.update_time as update_time,
        a.create_by as create_by,
        a.create_name as create_name,
        a.update_by as update_by,
        a.update_name as update_name,
        'application' as node_type,
        2 as level,
        a.domain_code as domain_code,
        a.code as application_code,
        NULL as module_code
    FROM public.cmx_application a
    WHERE a.status = 1 AND a.archived = 0

    UNION ALL

    -- 第三级：模块
    SELECT
        id as id,
        m.application_code as parent_code,
        m.code as code,
        m.name as name,
        m.description as description,
        m.type as type,
        m.tags as tags,
        m.icon as icon,
        m.title as title,
        m.sort_order as sort_order,
        m.status as status,
        m.archived as archived,
        m.create_time as create_time,
        m.update_time as update_time,
        m.create_by as create_by,
        m.create_name as create_name,
        m.update_by as update_by,
        m.update_name as update_name,
        'module' as node_type,
        3 as level,
        m.domain_code as domain_code,
        m.application_code as application_code,
        m.code as module_code

    FROM public.cmx_module m
    WHERE m.status = 1 AND m.archived = 0
)
SELECT
    id ,
    parent_code,
    code,
    name,
    description,
    type,
    tags,
    icon,
    title,
    node_type,
    level,
    domain_code,
    application_code,
    module_code,
    sort_order,
    status,
    archived,
    create_time,
    update_time,
    create_by,
    create_name,
    update_by,
    update_name
FROM domain_tree
ORDER BY
    CASE WHEN parent_code IS NULL THEN 0 ELSE 1 END,
    domain_code,
    application_code,
    sort_order;
