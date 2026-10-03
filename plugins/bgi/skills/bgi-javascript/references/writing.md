# 用真实宿主契约编写 JS

先确认确需 JS 行为、用户请求开发或解释已有脚本才进入本页；现成路线的组参数（队伍、战斗、拾取、恢复）先走原生配置组 `config.pathingConfig`，不为参数修改写脚本。遇到能力未暴露时先检查对象层级和完整 group config，不把索引没搜到当成 Native 不支持，也不默认造小脚本绕过。

先用 api.describe/read 的 `bgi.js_api.search` 和 `bgi.js_api.read` 查当前 EngineExtend 实际注入内容，不从 CLR 类名推导 JS 全局名。`settings` 是 settings_ui 的用户参数；file、http、genshin 等是对象；captureGameRegion、sleep 等是全局函数；RecognitionObject、ImageRegion、BvPage 等是注入类型。OpenCvSharp 是程序集类型集合，按完整命名空间查询具体类型。

`read` 保留构造器、静态与实例方法、继承成员、参数名、重载、可选默认值、ref/out、params、返回类型和枚举。返回对象也可继续按类型读取；只有明确注册的类型／命名空间类型有可用的全局构造入口。成员名绑定不区分大小写；Task 自动转 Promise，需要 await。不要把 namespace 查询结果、未加载的源码契约和已验证的运行时类型混成同一状态。

OCR／模板识别先读 captureGameRegion、RecognitionObject、ImageRegion、Region；新视觉接口读 BvPage、BvLocator、BvImage。核对 ROI 的像素／比例重载、1920×1080 游戏指标、截图实际尺寸、坐标转换、匹配阈值、空结果与图像生命周期。Find 和 FindMulti 的结果形状不同；结果区域的 Text、坐标、IsExist/IsEmpty、Click 等由 Region 契约提供。

一个只识别并记录文字的结构如下。先确认当前 read 返回 OcrThis、FindMulti、Text 与 Dispose；不要为识别示例偷偷加点击。

```js
(async () => {
  const frame = captureGameRegion();
  try {
    const matches = frame.findMulti(RecognitionObject.OcrThis);
    for (let i = 0; i < matches.Count; i++) {
      log.info(matches[i].Text);
    }
  } finally {
    frame.dispose();
  }
})();
```

区域识别、模板图、颜色、OpenCV 的参数按当前契约与实际资源补充；不要猜 Node.js、浏览器 DOM 或未注入 OcrFactory 全局。模块入口、library/packages 及 manifest.main 使用实际脚本加载器规则。

创建时写 manifest.json、main 和必要 settings_ui，参数选项／默认值完整对应代码。通过 bgi.user.write 保留版本和未知字段；准备与运行分开。先做源码／语法及非游戏检查，再按用户授权运行与读取日志、产物或截图结果。运行成功不能用没有执行过的所有分支替代。键鼠按下、定时器、图像／Mat 对象在 finally 中按各自真实契约收尾。
