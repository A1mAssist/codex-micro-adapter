# Codex Micro Adapter

[English](README.md) | 中文

用 Work Louder 的 **Codex Micro** 键盘配合任意编码 agent —— Claude Code、Codex
CLI、pi、opencode、DeepSeek Harness，或者任何跑在窗口里的东西。

ChatGPT/Codex 桌面端是用一套私有前端驱动这把键盘的。这个项目是一个独立的宿主，
说的是同一套设备协议，也复刻了同样的行为：六个 agent 键、键帽、摇杆、旋钮、麦克
风键，它们背后的灯光推导，以及配置这些的设置页。

```
desktop/               Tauri 应用：设置界面 + 内嵌宿主
backend/               Rust：HID 帧、JSON-RPC、布局、灯光、动作
plugins/codex-micro/   Claude Code + Codex CLI 插件，上报会话状态
plugins/pi/            pi 扩展：harness 事件 -> agent 键
plugins/opencode/      opencode 插件：harness 事件 -> agent 键
plugins/deepseek/      DeepSeek Harness 的 Cordis 插件：harness 接缝 -> 灯，
                       外加浏览器半边，按下键就打开对应会话
presets/               各 harness 的 Micro 键位映射
docs/HARNESSES.md      每个 harness 亮什么，按事件列出
docs/PROTOCOL.md       逆向出来的线上协议
```

## 复刻了什么

| 设备部件 | 行为 |
| --- | --- |
| 键帽 `ACT06`–`ACT12` | App 自己的键帽目录（38 个：`FAST`、`APPR`、`REJ`、`SPLIT`、`CODEX`、`GIT`、`YOLO` 等），默认值和 4×4 布局一致 |
| Agent 键 `AG00`–`AG05` | 六个状态灯；任何 harness 都能通过控制 socket 驱动 |
| 摇杆 | 四个方向，默认命令一致（`composer.togglePlanMode`、`navigateForward`、`toggleSidebar`、`navigateBack`），死区 `0.05` |
| 旋钮 | App 的四种模式：编辑器导航、推理档位、会话滚动、自定义分配 |
| 麦克风键 | 按住说话，包括 `ACT10`/`ACT11` 的合并/分离切换逻辑 |
| 灯光 | App 每次下发前跑的推导：`$` 会话灯、`se` 按键+氛围、`ce` 语音状态，配色也用 App 的 |
| 设置页 | 连接、电量、亮度、自动变暗、布局编辑器、旋钮/摇杆对话框、选项，用 App 自己的文案重建成了 Tauri 窗口 |

ChatGPT 专属的那部分逻辑（会话列表、"单击聚焦 Codex"、macOS 输入监控、App 的
命令注册表）被有意替换成控制 socket 和按键映射表，见[有意的差异](#有意的差异)。

## 安装

Windows 11 x64。到 [Releases](https://github.com/A1mAssist/codex-micro-adapter/releases)
下载安装包，普通安装用 `setup.exe`（NSIS），按策略部署用 `.msi`。宿主和设置窗口
都打包在里面了，只需要再装 Node.js，Claude Code 和 Codex CLI 的 hook 要用它。

装完之后再按你用的 agent 装上适配器，见[各 harness 的安装](#各-harness-的安装)。

## 构建与运行

Windows 11、Rust stable。Tauri 应用需要 MSVC 工具链；仓库里带了个辅助脚本，因为
在没装桌面版 CRT 的 Visual Studio 上直接 `cargo build` 会失败：

```powershell
. .\scripts\msvc-env.ps1        # 导入 vcvars64 并清掉 CC/CXX
cargo run -p codex-micro-desktop
```

控制台宿主（没有窗口，调起来方便）：

```powershell
cargo run -p codex-micro-backend -- run           # dry run：动作只记日志
cargo run -p codex-micro-backend -- run --live    # 真正注入按键
cargo run -p codex-micro-backend -- list          # 枚举 HID 接口
cargo run -p codex-micro-backend -- listen        # 只读：打印键盘发上来的东西
cargo run -p codex-micro-backend -- window        # 按 agent 键会聚焦到哪个窗口
```

哪里都是 dry run 优先：在应用里打开 **Send keystrokes**、或者在命令行上加
`--live` 之前，不会有任何东西打到别的窗口上。

测试：`cargo test`，覆盖帧解析、RPC、布局、灯光、动作、旋钮的点按/长按手势、
设备状态机和控制协议。各 harness 适配器有单独的自检：
`node scripts/check-harness-adapters.mjs` 会给 pi 扩展、opencode 插件、dsh 插件的
接缝和 dsh 的浏览器半边喂真实事件，再看它们发出去的行。

## 各 harness 的安装

宿主只认协议、不认 harness，但每个 agent 需要一个适配器把会话状态报上来。这里带
了五个。先把宿主跑起来（`codex-micro-desktop`，或者控制台的
`codex-micro-backend run --live`），再装你要用的那个 agent 对应的适配器。所有适配
器都连 `127.0.0.1:27700`，想换地方就设 `CODEX_MICRO_PORT`。

| Harness | 适配形式 | Agent 键 |
| --- | --- | --- |
| [Claude Code](#claude-code) | marketplace 插件 | 8 个 hook；`SessionEnd` 会把键释放掉 |
| [Codex CLI](#codex-cli) | marketplace 插件 | 7 个 hook；没信任的 hook 根本到不了 |
| [pi](#pi) | 扩展文件 | 扩展事件；关闭会话会释放键 |
| [opencode](#opencode) | 插件文件 | 会话 + 权限事件 |
| [dsh](#deepseek-harness-dsh) | 原生 Cordis 插件 | 接缝完整，含审批和会话结束；还能跳页面 |

五个 harness 的完整事件表在 [`docs/HARNESSES.md`](docs/HARNESSES.md)。

### Claude Code

```bash
claude plugin marketplace add A1mAssist/codex-micro-adapter
claude plugin install codex-micro@codex-micro-adapter
```

`SessionStart`、`UserPromptSubmit`、`PreToolUse`、`Notification`、`Stop`、
`PreCompact`、`SessionEnd` 会点亮该会话自己的键；hook 从 payload 里转发
`session_id`，键位由宿主分配，所以六个终端不用做任何 per-shell 配置
（`CODEX_MICRO_AGENT=0..5` 可以把某个终端钉死在某个键上）。需要 `PATH` 里有
Node.js。键位见 [`presets/claude-code.json`](presets/claude-code.json)：提交 `enter`、
批准 `enter`、拒绝 `escape`、计划模式 `shift+tab`。

### Codex CLI

```powershell
codex plugin marketplace add A1mAssist/codex-micro-adapter
codex plugin add codex-micro@codex-micro-adapter
```

第一次交互式启动会出现 **Hooks need review**，选 *Trust all and continue*。没被信任
的 hook 跑在 Codex 的沙箱里，够不到宿主，键就一直不亮；单次运行可以用
`codex exec --dangerously-bypass-hook-trust` 跳过这个提示。Codex 没有
`Notification` hook，所以让键变橙色的是 `PermissionRequest`。键位见
[`presets/codex-cli.json`](presets/codex-cli.json)：批准 `y`、拒绝 `d`、推理档位
`alt+,` / `alt+.`。

### pi

pi 没有 hook 配置，扩展 API 就是它的接口。

```powershell
New-Item -ItemType Directory -Force ~\.pi\agent\extensions
Copy-Item plugins\pi\codex-micro.ts ~\.pi\agent\extensions\
```

这样对所有项目生效；放在 `<repo>\.pi\extensions\` 则只对一个项目生效（需要项目
被信任）。会话开始、提交提示词、工具调用、审批弹窗、退出都会点亮键。键位见
[`presets/pi.json`](presets/pi.json)：提交 `enter`、批准 `enter`、拒绝 `escape`。

### opencode

opencode 在同进程里跑插件，从目录加载：

```powershell
New-Item -ItemType Directory -Force ~\.config\opencode\plugins
Copy-Item plugins\opencode\codex-micro.ts ~\.config\opencode\plugins\
```

项目里的 `.opencode\plugins\` 同理，只对一个项目生效。映射了
`session.created`、`session.idle`、`permission.asked`、`session.deleted`。键位见
[`presets/opencode.json`](presets/opencode.json)：提交 `enter`、中断 `escape`。

### DeepSeek Harness (dsh)

`dsh` 也有一套 Codex 风格的 hooks 桥，但那套桥会丢掉 `PermissionRequest`，也没有
`SessionEnd`，所以这里的适配器是原生的 Cordis 插件。它是个目录而不是发布到 npm 的
包，直接让你的 clone 上位：

```powershell
npx @deepseek-ai/dsh plugin --profile web add <REPO>/plugins/deepseek/plugin
```

```yaml
# %USERPROFILE%\.dsh\profiles\web\cordis.patch.yml
- insert:
    - id: codex-micro
      name: 'codex-micro-dsh'
```

`id` 是必填的，`dsh` 对裸写的 `- name:` 会直接报
`patch: id is required for non-insert patches`。如果你还在编辑器里用 ACP 驱动 `dsh`，
对 `acp` profile 重复一遍。

插件的浏览器半边是让 agent 键**在页面里切到对应会话**的东西，别的 harness 都不需要
这一步：`dsh` 既没有会话级 URL，也没有切换快捷键，所以由页面轮询宿主拿到刚按的
键，再调 `uiWorkspace.openSession()`。这个轮询就是宿主有 `GET /activation` 端点的
原因；要是你换了宿主的端口，记得改 `plugins/deepseek/plugin/client.js` 里的 `HOST`
常量。完整说明和限制见
[`plugins/deepseek/README.md`](plugins/deepseek/README.md)。

### 其它 harness

任何语言、任何 agent：往控制端口写 `session <id> <status>` 就是全部协议，
`session <id> end` 会把键释放掉。命令清单见下面的
[按键映射与控制 socket](#按键映射与控制-socket)。按 agent 键会聚焦该会话最后上报时
所在的窗口；完全没有窗口的 harness 可以改绑 `agent.focus.<n>`。

## 按键映射与控制 socket

两个方向，都跟具体 harness 无关。

**键盘 → harness。** 按键和摇杆通过 `%APPDATA%\codex-micro\config.json` 解析成动作：

```json
{
  "bindings": {
    "composer.submit": "enter",
    "approval.approve": "ctrl+enter",
    "forkThread": "type:/rewind",
    "OAI": "url:https://developers.openai.com"
  }
}
```

绑定语法刻意做得很小：`mod+mod+key`、`type:<字面文本>`、`url:<https 地址>`。没绑定的
动作会被报出来，绝不会被默默吞掉。默认值几乎为空是有意的 —— 替别人的工具发明键位
就是瞎猜。

**Harness → 键盘。** 宿主在 `127.0.0.1:27700` 上听换行分隔的命令，所以任何语言写的
hook、脚本或插件都能点亮 agent 键：

```bash
codex-micro-backend send "agent 0 working"          # 键 1 变蓝
codex-micro-backend send "agent 1 awaiting-approval" # 键 2 变橙
codex-micro-backend send "session 7f3a working"      # 宿主挑一个空键并记住归属
codex-micro-backend send "session 7f3a end"          # 把键还回去
codex-micro-backend send "voice recording"           # 氛围灯圈变蓝
codex-micro-backend send "brightness 40"
codex-micro-backend send "fleet error"               # 整圈，忽略单键状态
```

**Agent 键本身也是按钮。** 按第 N 个 agent 键，会把该会话最后上报时所在的窗口拉到
最前，并像 App 高亮你切过去的那条会话一样，把键点成选中态。宿主记的是会话**正在
启动或工作中**时前台是哪个窗口 —— 那一刻用户正在那里打字 —— 而且从不允许
`unread`/`awaiting` 事件覆盖它，所以后台会话抢不走别人窗口。没有已知窗口时（无头
运行，或者共用终端窗口的多个标签页），或者 Windows 拒绝了聚焦，按键会退回走
`agent.focus.<n>` 绑定，tmux 用户于是可以把它映射到自己的切换键上。
`codex-micro-backend window` 打印此刻会被聚焦的窗口，`window --focus <hwnd>` 则直接
试一次聚焦调用。

`session <id> <status>` 是 harness 插件要用的那条：它上报自己已有的会话 id，宿主
回一个它拿到的 agent 键（`ok session 7f3a agent 3 working`）。键按从低到高发放；六
个都占满时，最"不着急"的那个会易主 —— 顺序是 `off`、idle、unread，然后是等待和
工作状态，同状态里最旧的先换。手动的 `agent <n> …` 会把键从原来的会话手里拿回来。

状态词：`off`、`idle`、`working`、`unread`、`awaiting-approval`、
`awaiting-response`、`error`。

`activation` 返回 `{"seq":N,"session":"<id>"|null}`，也就是用户最后按下的那个 agent
键对应的会话，给能跳转过去的 harness UI 用。浏览器开不了 socket，所以同一个答案也
在控制端口的 `GET /activation` 上（同一端口的 HTTP，给只能说页面话的东西用）。两者
读的都是设备循环写入的槽，所以 USB 忙的时候轮询照样有回应（页面加载之前的按键不会
被重放：第一次回答只说明序号现在到哪了）。

## 有意的差异

从 App 移植过来，去掉的只是那些只有在 App 里才成立的部分：

- **Agent 键的来源**（固定 / 最近 / 优先会话）读的是 App 的会话存储。这里六个键显示
  的是插件通过 socket 上报的东西，没被分配的键保持熄灭。
- **编辑器导航**和**推理档位**两种旋钮模式调的是 App 的命令（`composer.*`）；换到别的
  harness 时，它们和别的动作一样走按键映射表。
- **单击聚焦**、**移除连接**，以及 macOS 的**输入监控**那一行，在独立宿主里没有对应物。
- 旋钮的**点按**和**长按**沿用 App 自己的表：`custom` 模式下触发
  `layout.encoder.click` / `.longPress`，内置模式下长按解析成 `settings` 命令，点按
  在那里留给 `encoder:click` 绑定。`encoder:press` / `encoder:release` 仍然是原始的
  按下事件，谁需要谁用。
- 键盘改键目前只有 Windows（`SendInput` + SetupAPI HID）；其余部分是平台无关的 Rust。

## 协议

`docs/PROTOCOL.md` 记录了 64 字节的 HID 帧、`{method, params, id}` 的 JSON-RPC 信封、
`v.oai.*` 方法和载荷形状、灯光推导，以及发现用的 ID。全部是从厂商自己的源码誊出来
的，不是猜的。

## 致谢

设备协议、键帽目录、布局规则和灯光推导都来自 ChatGPT 桌面端里随包发布的厂商源码
（`@worklouder/*`、`codex-micro-service`、`codex-micro-layout`、
`codex-micro-settings`）。这是一个独立、无隶属关系的宿主：这里没有重新分发任何
OpenAI 或 Work Louder 的代码，只是用它所用接口的 Rust 重实现。

MIT 许可。
