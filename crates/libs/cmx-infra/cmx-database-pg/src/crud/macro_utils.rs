// //! 生成Bmc/实体默认CRUD函数的便捷宏规则
// //! 注意：如果需要自定义功能，请使用以下代码作为基础代码
// //!       来实现自定义实现。
//
// #[macro_export]
// macro_rules! generate_common_bmc_fns {
// 	(
// 		Bmc: $struct_name:ident,
// 		Entity: $entity:ty,
// 		$(ForCreate: $for_create:ty,)?
// 		$(ForUpdate: $for_update:ty,)?
// 		$(Filter: $filter:ty,)?
// 	) => {
// 		impl $struct_name {
// 			$(
// 				/// 创建单个实体
// 				///
// 				/// # 参数
// 				/// * `ctx` - 上下文信息
// 				/// * `mm` - 模型管理器
// 				/// * `entity_c` - 要创建的实体数据
// 				///
// 				/// # 返回值
// 				/// 成功时返回新创建实体的ID，失败时返回错误
// 				pub async fn create(
// 					mm: &DatabaseManager, db_id: &str
// 					entity_c: $for_create,
// 				) -> Result<i64> {
// 					base::create::<Self, _>(ctx, mm, entity_c).await
// 				}
//
// 				/// 批量创建多个实体
// 				///
// 				/// # 参数
// 				/// * `ctx` - 上下文信息
// 				/// * `mm` - 模型管理器
// 				/// * `entity_c` - 要创建的实体数据列表
// 				///
// 				/// # 返回值
// 				/// 成功时返回新创建实体的ID列表，失败时返回错误
// 				pub async fn create_many(
// 					ctx: &Ctx,
// 					mm: &ModelManager,
// 					entity_c: Vec<$for_create>,
// 				) -> Result<Vec<i64>> {
// 					base::create_many::<Self, _>(ctx, mm, entity_c).await
// 				}
// 			)?
//
// 				/// 根据ID获取单个实体
// 				///
// 				/// # 参数
// 				/// * `ctx` - 上下文信息
// 				/// * `mm` - 模型管理器
// 				/// * `id` - 要获取的实体ID
// 				///
// 				/// # 返回值
// 				/// 成功时返回指定ID的实体，失败时返回错误
// 				pub async fn get(
// 					ctx: &Ctx,
// 					mm: &ModelManager,
// 					id: i64,
// 				) -> Result<$entity> {
// 					base::get::<Self, _>(ctx, mm, id).await
// 				}
//
// 			$(
// 				/// 根据过滤条件获取第一个匹配的实体
// 				///
// 				/// # 参数
// 				/// * `ctx` - 上下文信息
// 				/// * `mm` - 模型管理器
// 				/// * `filter` - 可选的过滤条件列表
// 				/// * `list_options` - 可选的列表选项
// 				///
// 				/// # 返回值
// 				/// 成功时返回第一个匹配的实体（如果没有匹配项则返回None），失败时返回错误
// 				pub async fn first(
// 					ctx: &Ctx,
// 					mm: &ModelManager,
// 					filter: Option<Vec<$filter>>,
// 					list_options: Option<ListOptions>,
// 				) -> Result<Option<$entity>> {
// 					base::first::<Self, _, _>(ctx, mm, filter, list_options).await
// 				}
//
// 				/// 根据过滤条件获取实体列表
// 				///
// 				/// # 参数
// 				/// * `ctx` - 上下文信息
// 				/// * `mm` - 模型管理器
// 				/// * `filter` - 可选的过滤条件列表
// 				/// * `list_options` - 可选的列表选项
// 				///
// 				/// # 返回值
// 				/// 成功时返回匹配的实体列表，失败时返回错误
// 				pub async fn list(
// 					ctx: &Ctx,
// 					mm: &ModelManager,
// 					filter: Option<Vec<$filter>>,
// 					list_options: Option<ListOptions>,
// 				) -> Result<Vec<$entity>> {
// 					base::list::<Self, _, _>(ctx, mm, filter, list_options).await
// 				}
//
// 				/// 根据过滤条件统计实体数量
// 				///
// 				/// # 参数
// 				/// * `ctx` - 上下文信息
// 				/// * `mm` - 模型管理器
// 				/// * `filter` - 可选的过滤条件列表
// 				///
// 				/// # 返回值
// 				/// 成功时返回匹配的实体数量，失败时返回错误
// 				pub async fn count(
// 					ctx: &Ctx,
// 					mm: &ModelManager,
// 					filter: Option<Vec<$filter>>,
// 				) -> Result<i64> {
// 					base::count::<Self, _>(ctx, mm, filter).await
// 				}
// 			)?
//
// 			$(
// 				/// 根据ID更新实体
// 				///
// 				/// # 参数
// 				/// * `ctx` - 上下文信息
// 				/// * `mm` - 模型管理器
// 				/// * `id` - 要更新的实体ID
// 				/// * `entity_u` - 更新的数据
// 				///
// 				/// # 返回值
// 				/// 成功时返回空结果，失败时返回错误
// 				pub async fn update(
// 					ctx: &Ctx,
// 					mm: &ModelManager,
// 					id: i64,
// 					entity_u: $for_update,
// 				) -> Result<()> {
// 					base::update::<Self, _>(ctx, mm, id, entity_u).await
// 				}
// 			)?
//
// 				/// 根据ID删除实体
// 				///
// 				/// # 参数
// 				/// * `ctx` - 上下文信息
// 				/// * `mm` - 模型管理器
// 				/// * `id` - 要删除的实体ID
// 				///
// 				/// # 返回值
// 				/// 成功时返回空结果，失败时返回错误
// 				pub async fn delete(
// 					ctx: &Ctx,
// 					mm: &ModelManager,
// 					id: i64,
// 				) -> Result<()> {
// 					base::delete::<Self>(ctx, mm, id).await
// 				}
//
// 				/// 根据ID列表批量删除实体
// 				///
// 				/// # 参数
// 				/// * `ctx` - 上下文信息
// 				/// * `mm` - 模型管理器
// 				/// * `ids` - 要删除的实体ID列表
// 				///
// 				/// # 返回值
// 				/// 成功时返回实际删除的实体数量，失败时返回错误
// 				pub async fn delete_many(
// 					ctx: &Ctx,
// 					mm: &ModelManager,
// 					ids: Vec<i64>,
// 				) -> Result<u64> {
// 					base::delete_many::<Self>(ctx, mm, ids).await
// 				}
// 		}
// 	};
// }
