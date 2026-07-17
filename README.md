# 书签匠

一个根据电子书目录页生成 PDF 书签的轻量 Windows 桌面应用。它不会修改原文件：用户选择目录页、校正书签与页码映射后，应用另存为 `原文件名_bookmarked.pdf`。

## 当前 MVP

- 导入并检测文本型、扫描型或混合型 PDF（抽样判断）
- 使用 PDF.js 按需渲染页面、可选择文本层和懒加载缩略图
- 手动选择目录页范围
- 按 PDF 版面坐标提取目录，兼容双栏、跨行标题、全角页码、引导符及标题/页码分离
- 可选 Windows 系统 OCR，处理扫描版或字体编码异常的目录页，无需下载额外模型
- 可手动填写 URL、API Key 和模型名，调用 OpenAI Chat Completions 兼容的多模态接口；密钥不落盘
- 自动读取 PDF 中已有的书签并载入编辑器
- 识别篇、章、节、中文序号、`1.1`、`Chapter`、`Part` 等常见层级
- 支持阿拉伯数字、罗马数字和常用中文数字页码
- 通过单锚点建立印刷页码到 PDF 页的映射
- 编辑标题、目标页和层级；新增、删除、拖动排序、撤销和重做
- 对未映射条目同时显示文字状态和颜色提示
- 将已映射书签安全写入新的 PDF；未映射条目会被跳过并明确提示
- PDF 连续滚动预览，画布与可选文字层按视口懒加载

## 启动

需要 Node.js 20+、Rust stable、Windows WebView2 和 Tauri 2 的系统依赖。

```powershell
npm install
npm run tauri dev
```

只启动浏览器界面（文件命令不可用）：

```powershell
npm run dev
```

## 构建与测试

```powershell
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build
```

安装包由 Tauri 写入 `src-tauri/target/release/bundle/`。

## 使用流程

1. 点击“导入 PDF”。
2. 在顶部填写目录的 PDF 起止页，点击“提取目录”。
3. 设置锚点，例如“印刷页 1 对应 PDF 页 13”，应用映射。
4. 在右侧检查标题、层级与目标页；双击目标页输入框可跳转预览。
5. 点击“导出带书签 PDF”；未映射条目不会阻塞其他有效书签导出。

导入本身已有书签的 PDF 时，右侧会直接显示原书签树，无需重新识别目录。

输出默认命名为 `原文件名_bookmarked.pdf`。应用禁止覆盖原文件，也拒绝覆盖已存在的输出文件。

## 架构

```text
src/
├─ components/              PDF 预览、懒加载缩略图、书签编辑器
├─ composables/             书签状态与撤销/重做
├─ App.vue                  MVP 工作流编排
└─ types.ts                 前后端共享形状的 TypeScript 类型

src-tauri/src/
├─ models.rs                Tauri 命令输入输出结构
├─ pdf.rs                   文件校验、文本提取、书签写入和原子导出
├─ toc.rs                   目录规则解析、层级推断和页码映射
└─ lib.rs                   异步 Tauri 命令边界
```

更详细的数据流与边界见 [docs/architecture.md](docs/architecture.md)。

## 主要技术决策

- Tauri 2 + Vue 3 + TypeScript：Windows 安装体积小，编辑界面开发直接。
- PDF.js：前端按页渲染，缩略图进入视口附近才生成。
- lopdf：MVP 不依赖 Python 运行时即可提取文本并写入书签。
- 规则优先：目录解析和页码映射完全离线、可复现，不依赖 AI。
- 原子导出：先在输出目录写临时文件，成功后重命名；原始 PDF 只读。
- 动态文件权限：Tauri 仅把用户本次选择的 PDF 加入预览协议权限，不开放整个磁盘。

## 尚未实现

- 扫描目录 OCR、双栏和精确坐标版面分析
- 自动推荐目录页、自动识别正文印刷页码
- 多锚点分段映射、书签 JSON 导入导出
- AI 低置信度修正、任务取消与持久化缓存
- 合并两套书签树（当前会载入并编辑已有书签，导出时以编辑器内容替换原书签树）

## 已知问题

- `lopdf` 对采用特殊字体编码的 PDF 可能无法正确提取中文；扫描型 PDF 会明确提示无法识别目录。
- MVP 的 `x/y/width/height` 是基于文本行的近似值，不适合双栏目录；引入 OCR/版面引擎后再提供真实坐标。
- PDF.js 主包约 534 KB（gzip 后约 166 KB），桌面应用中可接受；没有为消除构建警告增加分包配置。
- 缩略图按需渲染，但页面列表 DOM 仍与页数线性增长；数千页文档若出现滚动问题，再加入虚拟列表。

## 下一步

优先加入“只对目录页 OCR + 双栏排序”，随后实现多锚点映射。这两项能覆盖当前 MVP 最主要的失败场景，不必先接入 AI。
