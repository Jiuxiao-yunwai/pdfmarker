# 开发流程约定

书签匠使用两个长期分支：`main` 和 `develop`，不设特性分支。

## 分支职责

- `develop`：日常开发分支。所有开发版、小版本迭代都在此分支上进行，每个小版本完成后 commit 一次。
- `main`：正式版分支。只接受发布正式版时的合并，日常开发不碰。

## 日常开发

1. 在 `develop` 上修改代码。
2. 完成一个开发小版本后，更新 `version.json`、`CHANGELOG.md`，运行构建，然后 `git commit` 一次。
3. 不需要发布正式版时，始终停留在 `develop`，不合并 `main`。

## 发布正式版

只在用户明确要求"发布正式版"时执行：

1. 确认 `develop` 上待发布的开发版全部完成并已 commit。
2. 切换到 `main`，将 `develop` 合并进来（`--no-ff`）。
3. 同步正式版元数据并生成安装包。
4. 打版本 tag，推送到远程。

## 日常操作命令

```bash
git switch develop        # 日常开发
git switch main && git merge --no-ff develop && git push  # 发布正式版
```
