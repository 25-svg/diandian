# Agency agents（本仓库）

自定义人设源文件，格式对齐 [Agency Agents](https://agencyagents.app)。

| 文件 | Cursor 规则 | 用法 |
|------|-------------|------|
| `design-ui-workspace-manager.md` | `.cursor/rules/design-ui-workspace-manager.mdc` | 对话里 `@UI Workspace Manager` |

## 怎么用

1. Cursor Agent 输入 `@UI` 或 `@Workspace`，选中 **UI Workspace Manager**
2. 说明布局问题，例如：`整场复盘要左文稿右优化，现在叠在一起了`
3. 改完按规则验收：点进对应 tab，看左右栏是否符合 canon

同步：改 `agent/agency/*.md` 后，请同步更新 `.cursor/rules/` 同名 `.mdc`（或再导入一次）。
