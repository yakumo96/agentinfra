# 当前状态

更新：2026-11-02。对应实现文档的“代码仓库结构与技术栈”一节。

## 已完成（M0：契约与骨架）

| 部分 | 位置 | 验证 |
| --- | --- | --- |
| 工具接口契约：33 个 MCP 工具、18 个错误码、43 个共享定义 | `schemas/` | `tools/schema/validate.py`：元 schema、ID 与幂等键规则、`$ref` 可解析、示例正反例 |
| 三种语言的生成代码 | `infra/crates/proto/src/generated.rs`、`harness/tools/src/generated/`、`tests/lib/` | `codegen.py --check`；Rust 测试、`tsc --strict` 通过 |
| 控制面迁移：全部表 + 原子操作函数 | `migrations/0001_init.sql` | `tools/db/test.sh` 在临时 PostgreSQL 16 上跑 6 组测试，含 12 个并发认领者抢 60 个工作项 |
| `ws_apply` 语义：先全部校验、后写入；路径安全与符号链接逃逸；pi 的多处 edit | `infra/crates/fileops` | 10 个测试 |
| 合并队列决策：批大小 1→8、按顺序折叠、证据绑定 T、条件发布、二分定位 | `infra/crates/publish` | 6 个测试 |
| 工作项路径重叠（保守的 glob 交集） | `infra/crates/workitem` | 3 个测试 |
| harness 四个内置工具到 Infra 的映射，MCP Streamable HTTP 客户端 | `harness/tools` | 7 个测试 |
| Gateway 上下文保护规则、统一原文存储、压缩阶段 | `gateway/` | 5 个测试 |
| LayerFS v1.1.0 吸收：14 个 crate 改名进入 `infra/crates/`，fuser 补丁随附 | `infra/UPSTREAM.layerfs.toml`、镜像分支 `upstream/layerfs` | 1126 个源文件解析通过；`check_absorbed.py` 校验全部清单 |

## 受阻

- **软件包源不可达**：本次构建环境访问不了 crates.io、npm、PyPI。因此吸收来的 LayerFS crate 暂不进 workspace 成员（`Cargo.toml` 的 `exclude`），
  也还没跑它们自带的测试；需要 tokio、MCP SDK、PostgreSQL 客户端的 `infrad` 也暂未开写。放开后的第一步：把 `exclude` 里的 crate 移进
  `members`，加上 `[patch.crates-io] fuser`，跑 `cargo test`，按结果修吸收改名带来的问题。

## 对实现文档的修订

- `ws_apply` 增加 `edit` 操作，语义与 pi 当前的 edit 工具一致（多处替换都对照原文匹配、不得重叠），harness 的 edit 映射到它。
- `ws_read`、`rev_read` 的 `offset`、`limit` 按行计（与 pi 的 read 一致），输出带 `total_lines`。
- 新增错误码 `INVALID_ARGUMENT`、`NOT_FOUND`、`UNSUPPORTED_OPERATION`、`OPERATION_IN_PROGRESS`。
- `proposal_submit` 返回 `candidate_commit`（候选就是分支头），revision 在发布时才生成。
- 新增 `checkpoints` 表（`ws_checkpoint` 的命名标记）。
- 吸收基线从 LayerFS 1.0 改为 1.1.0。

## 下一步（M1：单 agent 闭环）

1. 放开软件包源后，吸收来的 crate 编译通过并跑通上游测试。
2. `store-sqlite` 的 Durable 模式（UPSTREAM 改造 `durable-profile`）。
3. `infrad`：MCP 端点、令牌、幂等、调用 `migrations` 里的函数；`ws_read`、`ws_apply`、`ws_status` 先接物化投影。
4. `infra-sandboxd` 搬到宿主机，接 LXC（`infra/crates/sandbox`，liblxc FFI）与 FUSE passthrough。
5. harness 吸收 pi 的 agent 循环，跑通“认领 → 改 → 测 → 提交”。
