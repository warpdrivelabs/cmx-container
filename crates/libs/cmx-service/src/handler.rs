// //! HTTP Handler
// //!
// //! 提供 cmx-api 可调用的 HTTP 处理器，封装服务层逻辑。
//
// use crate::request::{InvokeRequest, InvokeResponse};
// use crate::service::CmxService;
//
// /// 服务处理器
// ///
// /// 封装 CmxService，提供统一的 HTTP 处理入口。
// pub struct ServiceHandler {
//     /// 核心服务
//     service: CmxService,
// }
//
// impl ServiceHandler {
//     /// 创建新的服务处理器
//     ///
//     /// # 参数
//     ///
//     /// * `service` - 核心服务实例
//     pub fn new(service: CmxService) -> Self {
//         Self { service }
//     }
//
//     /// 获取核心服务引用
//     pub fn service(&self) -> &CmxService {
//         &self.service
//     }
//
//     /// 处理单次调用请求
//     ///
//     /// # 参数
//     ///
//     /// * `request` - 调用请求
//     ///
//     /// # 返回值
//     ///
//     /// 返回调用响应。
//     pub async fn handle_invoke(&self, request: InvokeRequest) -> InvokeResponse {
//         match self.service.invoke(&request).await {
//             Ok(response) => response,
//             Err(e) => InvokeResponse {
//                 success: false,
//                 output: None,
//                 elapsed_us: 0,
//                 fuel_consumed: 0,
//                 error: Some(e.to_string()),
//             },
//         }
//     }
// }
