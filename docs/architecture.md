# 架构说明

## 边界

前端只保存用户正在编辑的书签数据，不读取任意本地路径。Rust 文件边界负责验证扩展名、规范化路径、检查页码和写入结果。用户选中的单个 PDF 才会动态加入 Tauri asset protocol 权限，供 PDF.js 按页读取。

## 数据流

```text
文件选择
  -> Rust 校验与抽样检测
  -> PDF.js 按页预览
  -> 用户选择目录页范围
  -> Rust 裁出仅包含所选页面的临时内存 PDF
  -> Responses API 以 input_file + 提示词识别目录并按 JSON Schema 返回
  -> 若 PDF/Responses 文件输入不兼容，PDF.js 将所选页渲染为高清 PNG
  -> Chat Completions API 以多张 image_url 一次识别同一目录范围
  -> Rust 解析、校验并修正为扁平书签序列（level 表示树层级）
  -> 单锚点页码映射
  -> Vue 编辑与历史记录
  -> Rust 完整校验
  -> 临时文件写入
  -> 原子重命名为 *_bookmarked.pdf
```

扁平书签序列比递归状态更适合排序和层级调整。导出时 Rust 使用 `level` 栈恢复 PDF outline 父子关系；`children` 字段保留在接口中，当前 MVP 为空。

## 长任务

PDF 导入检测、目录页裁剪和导出通过 `spawn_blocking` 离开 Tauri 异步调度线程。PDF.js 在 Web Worker 中解析文档；连续主预览与缩略图均使用 `IntersectionObserver` 懒加载。API 调用设置超时、响应大小限制和最多三次重试。识别命令同时返回 Token usage、API 耗时和传输方式，前端另行统计包含截图与映射在内的完整流程用时。

高清截图回调使用相对进度 `completed / total`，不直接使用原 PDF 页码。AI 请求期间使用不确定进度动画；完成或失败后由 Rust 使用固定 AppUserModelID 发送 Windows 原生通知。应用启动时在当前用户注册表中注册“书签匠”显示名称，使安装版和免安装版通知都不再归属 PowerShell。

## 错误与数据安全

- 输入必须是存在的 `.pdf` 文件。
- 加密文档在 MVP 中直接拒绝，不尝试绕过权限。
- 只上传用户选择的目录页子集，子集上限为 45 MB；降级时只上传同范围的高清截图，原文件路径不会发送给 API。
- 提取页范围、API 返回结构、映射锚点、标题、目标页和层级都在 Rust 边界校验。
- 输出不得等于输入，也不得覆盖已有文件。
- 写入失败时删除本次创建的临时文件；日志不记录正文或目录全文。

## 演进位置

如果后续需要兼容不支持 Responses API 的供应商，应在 `vision.rs` 增加独立适配器，不让供应商协议进入 Vue。多锚点映射仍只需扩展 `toc.rs`，无需改动 AI 识别或 PDF 写入。
