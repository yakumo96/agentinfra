# Infra 使用规则

这份说明随 `instructions_resolve` 发给每个 agent，排在各作用域的 AGENTS.md 之前。

## 你在哪里工作

- 你在一个**工作区**里工作：它挂在一条分支上，基础是某个 `main` revision。容器里的路径是 `/workspaces/<workspace_id>`。
- 每次改动文件的工具调用（write、edit、bash）结束时，改动自动提交成一个 commit，不用你手动保存。
- 别的 agent 看不到你的工作区，你也看不到它们的。要用别人已经合并的工作，就 `ws_rebase` 到新的 `main`。

## 文件

- 路径一律用工作区内的相对路径，或 `/workspaces/<workspace_id>/` 下的绝对路径。`..`、其他绝对路径、指向工作区外的符号链接都会被拒绝（`PATH_ESCAPE`）。
- edit 的每处旧文本必须在文件里恰好出现一次；不唯一时扩大上下文再试。
- 一次 write 或 edit 里的多处修改要么全部生效，要么全部不生效。
- 构建输出目录（项目在 `.agentinfra/workspace.toml` 里声明）不进历史；依赖目录进历史。

## 命令

- bash 在你自己的容器里执行，网络只能经出口代理访问白名单里的域名。
- 后台进程（开发服务器等）跨提交继续运行。
- 输出过长时会截断，完整内容用 `artifact_read` 取。

## 协作

- 主线只能经合并队列推进。你提交 Proposal，队列负责合并、在一次性容器里验证、发布。
- 你自己跑过的测试只是参考；只有队列对合并结果的验证能当发布证据。
- Proposal 被 BLOCKED 时（冲突或验证失败），看 `proposal_status` 的原因，`ws_rebase` 到新主线，修好后重新提交。
- 改动 `.agentinfra/**`、环境 Dockerfile、CI 配置需要人审批。

## 出错时

错误带 `code` 和 `retryable`。`retryable: true` 的可以稍后用同样的参数重试；其余的说明你需要先改变做法：
`REVISION_MISMATCH` 先重新读，`WORKSPACE_BUSY` 等待或 fork，`FENCED_OUT` 停下来查 `ws_status`，`LEASE_EXPIRED` 重新认领。
