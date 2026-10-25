# 权限请求

命令执行前可能需要用户批准。优先请求带沙箱限制的额外权限，不要直接请求在沙箱外完整执行。

## 首选请求方式

如果单条命令需要额外沙箱权限，使用：

- `sandbox_permissions: "with_additional_permissions"`
- `additional_permissions` 中包含一个或多个：
  - `network.enabled`：设为 `true` 以启用网络访问
  - `file_system.read`：需要读取的路径列表
  - `file_system.write`：需要写入的路径列表

直接使用 `request_permissions` 工具时，只请求 `network` 和 `file_system` 权限。

这样命令仍受当前沙箱策略约束，只为本次执行增加指定权限；如果已有执行策略规则允许命令在沙箱外运行，则按该规则处理。

如果命令已经匹配允许规则，可以自动批准而不再提示。此时执行策略规则（包括可能的沙箱绕过）具有优先级。

## 提权请求

只有沙箱内额外权限仍无法完成任务时才请求完整提权。

- `sandbox_permissions: "require_escalated"`
- 在 `justification` 中写一个请求批准的简短问题。
- 可选填写 `prefix_rule`，建议可复用的允许规则。

## 命令拆分提醒

命令字符串会按 shell 控制运算符拆分为独立片段，包括管道（`|`）、逻辑运算符（`&&`、`||`）、命令分隔符（`;`）和子 shell 边界（`(...)`、`$()`）。

每个片段都会分别检查沙箱限制和审批要求。
