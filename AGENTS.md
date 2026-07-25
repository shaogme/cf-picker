# Coding Guidelines and Instructions for Agents

When making modifications to this repository, please adhere to the following strict requirements. Failure to run and pass these checks will result in a Continuous Integration (CI) failure.

**IMPORTANT:** Always use Simplified Chinese (简体中文) when communicating and providing explanations.

## 核心原则 (Core Principles)

1. **回复语言**：始终使用**中文**回复。
2. **代码风格**：
   - **严禁使用 `mod.rs`**。必须遵守 Rust 2018 Edition 及更新版本的目录结构标准。
   - 模块 `foo` 应定义在 `foo.rs` 中；若有子模块，创建 `foo/` 目录，但父模块代码仍保留在 `foo.rs`，而非 `foo/mod.rs`。
3. **禁止猜测**：严禁猜测代码逻辑或文件内容；修改或回答前必须先读取相关代码。
4. **主动报告**：阅读代码时应主动报告潜在错误、安全漏洞、性能问题。
5. **绝对路径**：使用文件修改工具时（如 `write_to_file`、`replace_file_content`），**必须**使用**绝对路径**。
6. **Rust Edition 2024**：充分利用 Rust 2024 新特性，特别是异步闭包和 `AsyncFnOnce` / `AsyncFnMut` / `AsyncFn`，避免手动装箱 `Future`。

## 代码质量要求

- **质量与测试**: 注重代码质量、可测试性和测试覆盖。
- **编码规范**:
    - **禁止长路径**: 禁止在代码中使用全限定命名空间（尤其是以 `crate::` 开头的路径）超过 15 个字符。必须通过 `use` 语句导入后再调用。
    - **合并相同前缀的use语句**: 当有多个`use`语句具有相同前缀时，应合并为一条`use`语句，例如：
    ```rust
    //Bad
    use crate::nix::build;
    use crate::nix::store;
    use crate::nix::path;
    use crate::nix::refpath;
    //Good
    use crate::nix::{
        build,
        store,
        path,
        refpath,
    };
    ```
    - **cfg属性分组与换行隔离**: 所有 `#[cfg(...)]` 内的条件相同的use语句必须放在一起，并且与其他不同条件或不带 `cfg` 的use语句显式使用空行分隔。
    - **禁止在use嵌套导入内使用cfg**: 禁止在 `use {...}` 的 `{...}` 内部使用 `#[cfg(...)]`。

## CI 命令风格 (CI Command Style)

- 禁止在 CI 中使用批处理循环/脚本进行重试或编排。
- 统一使用 `.cargo/config.toml` 中的 `x*` 别名入口。
- 平台差异应收敛在 Rust/Cargo 配置与 `xtest-runner` 内，不要散落在 shell 脚本中。
- 不要为跨平台编译引入包装/存根入口文件；平台后端 crate 保持其目标平台原生形态。

如任一检查未通过，必须先修复再提交。

## Git 提交信息 (Commit Message) 规范与指南

本指南旨在规范项目中 Git 提交信息（Commit Message）的格式和内容要求，以便于生成清晰的变更历史、简化代码审查过程以及维护高质量的系统演进记录。

---

### 1. 提交信息结构

每次提交信息应当包含一个简短的**标题（Header）**，以及（对于复杂或重大的变更）详细的**正文（Body）**。

#### 1.1 标题格式 (Header Format)

标题必须控制在单行内，推荐格式如下：
```text
<type>(<scope>): <subject>
```

- **`<type>`（类型）**：描述本次变更的性质，必须使用小写。常用类型包括：
  - `feat`: 新增功能（Feature）。
  - `fix`: 修复 Bug。
  - `refactor`: 代码重构（既不修复 Bug 也不添加新功能，如修改能见度、代码结构优化等）。
  - `perf`: 性能提升（Performance）。
  - `style`: 格式化、缺失分号等不影响代码运行的变更。
  - `test`: 新增或修改测试代码。
  - `chore`: 构建过程或辅助工具、库的变动。
- **`<scope>`（范围）**：可选，描述本次变更影响的子系统或模块，必须使用小写。例如：
  - `sync`: 异步同步原语（如通道、队列）。
  - `runtime`: 运行时底层设计（如调度器、工作线程）。
  - `scope`: 结构化并发作用域（Scope）。
  - `completion`: 完成队列与异常传播（Completion Anomaly）。
  - `driver` / `driver-core`: 驱动层与底层 I/O 后端（如 IOCP、io_uring）。
- **`<subject>`（主题说明）**：简短描述本次变更的核心内容。
  - 使用祈使句/现在时（例如，使用 `unify` 而不是 `unified`，使用 `slim` 而不是 `slimmed`）。
  - 结尾不加句号。

*示例：*
- `refactor(sync): unify queue abstractions and simplify mpsc bounded strategy`
- `perf(scope): avoid unnecessary heap allocation in RoutedJobCell::take`
- `feat(runtime): redesign select! macro to support fair and biased polling`

---

### 2. 正文格式 (Body Format)

对于重构（`refactor`）、性能优化（`perf`）、新特性（`feat`）等复杂改动，**必须**在标题下方空一行后写入详细的正文，清晰说明改动的背景、设计决策、API 变动以及兼容性影响。

正文应当按以下结构或内容进行组织：

#### 2.1 背景与动机 (Motivation & Context)
简要描述为什么要进行此改动。例如，某个结构体在热路径上占用内存过大（如 `CompletionAnomaly`），导致频繁拷贝造成栈空间浪费或性能瓶颈。

#### 2.2 核心设计与修改内容 (Core Design & Key Changes)
清晰列出修改的要点，例如：
- 引入了哪些新的轻量级类型（如 `CompletionAnomalyKind`）来替代胖结构体。
- 变更了哪些核心逻辑或边界策略。
- 调整了哪些方法或类型的可见性（例如将内部结构调整为 `pub(crate)`）。

#### 2.3 接口变动与破坏性改动 (API Changes & Breaking Changes)
如果变更会导致 API 不兼容，必须明确指出：
- 哪些 API 方法被移除或重命名。
- 参数或返回值类型的改变。
- 依赖项的更新。

#### 2.4 测试与后端更新 (Test & Backend Updates)
描述配套修改了哪些后端实现（如 `iocp`、`uring` 等）或单元测试、Loom 并发测试，以确保整体编译和测试的通过。

---

### 3. 良好实践与典型示例

#### 典型示例 1：核心重构与类型轻量化
```text
refactor(driver): slim completion anomaly propagation with CompletionAnomalyKind

Introduce a lightweight propagation layer for completion anomalies so hot
paths (mutation, table, routing, poll) carry ~24–40 B kinds instead of full
~72 B CompletionAnomaly values. Full anomalies are materialized only at
explicit boundaries where token/raw context is available.

Core type changes (driver/core):
- Add CompletionAnomalyKind, AnomalyAttach, AnomalyOutcome, ControlAnomalyReason,
  SlotIssueReason, and BackendSlotRef.
...
```

#### 典型示例 2：可见性收紧
```text
refactor(runtime): restrict visibility of internal types and methods to crate-local

Restrict the visibility of internal runtime and task subsystem methods and
structures to `pub(crate)` to improve encapsulation and modularity within the
`veloq-runtime` crate.

- **scope/completion.rs**: Change GenericScopeCompletion methods to pub(crate).
- **task/header.rs**: Restrict internal state constants to pub(crate).
...
```

#### 典型示例 3：错误传播改造（API 破坏性改动）
```text
refact: change `Runtime::block_on` to return `Result` and propagate errors

Refactors `veloq-runtime`'s `Runtime::block_on` to return a `Result<R>` instead
of returning the result directly (panicking on internal initialization failures).
This allows propagating thread-local storage (TLS) setup failures, worker factory
taking errors, and deque exhaustion errors gracefully back to the caller.
...
```
