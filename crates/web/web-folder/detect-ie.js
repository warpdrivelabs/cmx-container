/**
 * @Author: zhangqin
 * @Date: 2026-03-06 13:49:54
 * @Description:
 */
// Tip: Simple judgments may not fully cover
if (/MSIE\s|Trident\//.test(navigator.userAgent)) {
  document.body.innerHTML = "<strong>抱歉，当前浏览器版本不受支持。我们建议使用最新版本的现代浏览器，例如Chrome、Firefox或Edge。</strong>"
}
