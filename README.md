# agentinfra

单机版 Agent Infrastructure：Infra、Gateway、Harness 三层分离，让多个 agent 同时推进同一个项目。
设计见《Agent Infrastructure 单机版实现文档》。

| 层 | 目录 | 语言 | 进程 |
| --- | --- | --- | --- |
| Infra：工作区、版本、执行、工作项、合并队列 | `infra/` | Rust | `infrad`、`infra-sandboxd`、`infractl` |
| Harness：agent 循环、四个内置工具 | `harness/` | TypeScript（Node ≥ 22.18） | `harnessd`、`agentctl` |
| Gateway：模型路由、预算、上下文保护与压缩 | `gateway/` | Python ≥ 3.12 | `gatewayd` |

三层之间只通过 MCP（Infra，`127.0.0.1:7070/mcp`）和模型 API（Gateway）通信，不互相引用代码。
接口唯一来源是 `schemas/`，三种语言的类型都由 `tools/schema/codegen.py` 生成。

## 目录

```
infra/crates/      Rust crate；吸收自 LayerFS 的在 UPSTREAM.layerfs.toml 里登记
infra/bin/         infrad、infra-sandboxd、infractl
harness/           core、seams、server、tools、agentctl（pnpm workspace）
gateway/           gatewayd（uv 项目）
schemas/           common/（信封、错误码）与 tools/<分组>/<工具>.json
migrations/        PostgreSQL 迁移，含原子操作函数
instructions/      发给 agent 的 Infra 使用规则
prompts/           协调者与工作者提示词
tools/             schema 校验与代码生成、上游吸收脚本、迁移测试
third_party/       第三方许可证与 NOTICES
deploy/systemd/    单机部署单元
tests/             e2e、storage、fault、security、conformance-pi
```

## 常用命令

根目录的 `justfile` 是唯一入口：

```sh
just check        # schema 校验、代码生成一致性、cargo check、tsc、python 编译检查
just test         # 各语言单元测试 + 迁移测试（需要本机 PostgreSQL 16）
just codegen      # 从 schemas/ 重新生成三种语言的类型
just absorb-layerfs  # 按 UPSTREAM.layerfs.toml 从 upstream/layerfs 分支重新吸收
```

## 上游

参考项目是来源，不是依赖。每个上游一个只读镜像分支 `upstream/<项目>`，从不合并进 `main`；
吸收映射、我们的改动和拒绝的上游变更都记在 `infra/UPSTREAM.<项目>.toml`。

## 当前状态

见 `docs/STATUS.md`。

## 许可证

MIT，见 `LICENSE`。吸收的第三方代码及其许可证见 `third_party/NOTICES.md`。
