pub mod args;
pub mod io_utils;
pub mod ui;

use std::{
    cmp::{max, min},
    io::stdin,
    time::Duration,
};

#[cfg(target_env = "musl")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

use colored::Colorize;
use comfy_table::{
    Attribute, Cell, CellAlignment, Color, ContentArrangement, Table, presets::UTF8_FULL,
};
use tokio::{select, spawn, sync::mpsc::unbounded_channel, time::interval};

use args::CliArgs;
use cf_picker_core::{
    DownloadEvent, DownloadRunner, NetworkError, PingEvent, PingRunner,
    models::{CloudflareIpData, PickerOptions},
};
use cf_picker_tokio::Client;
use io_utils::{RandImpl, export_csv, load_ip_ranges};
use ui::{ProgressStage, TerminalUi, UiEvent};

const VERSION: &str = "0.1.0";

#[tokio::main]
async fn main() {
    let args = CliArgs::load();

    if args.version {
        println!("{VERSION}");
        return;
    }

    let options = PickerOptions::from(&args);

    let ips = match load_ip_ranges(&args.ip_text, &args.ip_file, args.test_all) {
        Ok(ips) => ips,
        Err(e) => {
            eprintln!("{}", format!("[错误] 读取 IP 数据失败: {e}").red());
            return;
        }
    };

    if ips.is_empty() {
        println!("{}", "[提示] 没有发现任何要测速的 IP！".yellow());
        return;
    }

    let client = Client::new();

    let mode_name = if options.httping { "HTTP" } else { "TCP" };
    let ping_info = format!(
        "开始延迟测速（模式：{}, 端口：{}, 范围：{} ~ {} ms, 丢包：{:.2}）",
        mode_name,
        options.tcp_port,
        options.min_delay.as_millis(),
        options.max_delay.as_millis(),
        options.max_loss_rate
    );

    let header_msg = format!("VERSION: {VERSION}");
    let (ui_tx, mut ui_rx) = unbounded_channel::<UiEvent>();

    let _ = ui_tx.send(UiEvent::SetStage {
        stage: ProgressStage::Ping,
        sub_header: Some(ping_info.clone()),
        total: ips.len() as u64,
    });

    // 启动无锁后台渲染任务
    let ui_handle = spawn(async move {
        let mut terminal_ui = match TerminalUi::new(header_msg) {
            Ok(ui) => ui,
            Err(e) => {
                eprintln!("{}", format!("[错误] 初始化终端 UI 失败: {e}").red());
                return (Vec::new(), false);
            }
        };

        let mut interval = interval(Duration::from_millis(100));

        loop {
            select! {
                maybe_event = ui_rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            terminal_ui.handle_event(event);
                        }
                        None => break,
                    }
                }
                _ = interval.tick() => {
                    let _ = terminal_ui.draw();
                }
            }
        }

        let _ = terminal_ui.draw();
        let _ = terminal_ui.cleanup();
        (terminal_ui.take_errors(), true)
    });

    let ping_runner = PingRunner::new(options.clone());
    let mut rng = RandImpl;
    let is_debug = options.debug;

    let ping_tx = ui_tx.clone();
    let ping_step = max(1, options.routines / 10);
    let ping_results = ping_runner
        .run(
            &ips,
            &client,
            Some(move |event| match event {
                PingEvent::Progress {
                    completed,
                    available,
                    ..
                } => {
                    let _ = ping_tx.send(UiEvent::UpdateProgress {
                        pos: completed as u64,
                        available: Some(available),
                    });
                    if completed % ping_step == 0 {
                        let msg = format!(
                            "[测速] 已完成 {} 个 IP 延迟测试 (可用: {})",
                            completed, available
                        );
                        let _ = ping_tx.send(UiEvent::AddLog(msg));
                    }
                }
                PingEvent::Error { ip, error } => {
                    if is_debug || !matches!(error, NetworkError::Timeout) {
                        let msg = match ip {
                            Some(addr) => format!("[错误] IP {addr} 延迟测速异常: {error}"),
                            None => format!("[错误] 延迟测速异常: {error}"),
                        };
                        let _ = ping_tx.send(UiEvent::AddError(msg));
                    }
                }
                PingEvent::Finished(_) => {}
            }),
            &mut rng,
        )
        .await;

    let download_info = if ping_results.is_empty() || options.disable_download {
        None
    } else {
        let test_num = if ping_results.len() < options.test_count || options.min_speed > 0.0 {
            ping_results.len()
        } else {
            options.test_count
        };
        let target_count = min(options.test_count, test_num);
        let min_speed_mb = options.min_speed / 1024.0 / 1024.0;
        Some((
            format!(
                "开始下载测速（下限：{:.2} MB/s, 数量：{}, 队列：{}）",
                min_speed_mb, target_count, test_num
            ),
            target_count,
        ))
    };

    let speed_results = if let Some((dl_msg, target_count)) = download_info {
        let _ = ui_tx.send(UiEvent::SetStage {
            stage: ProgressStage::Download,
            sub_header: Some(dl_msg),
            total: target_count as u64,
        });

        let download_runner = DownloadRunner::new(options.clone());
        let dl_tx = ui_tx.clone();

        download_runner
            .run(
                ping_results,
                &client,
                Some(move |event| match event {
                    DownloadEvent::Progress { current, total: _ } => {
                        let _ = dl_tx.send(UiEvent::UpdateProgress {
                            pos: current as u64,
                            available: None,
                        });
                    }
                    DownloadEvent::ItemSuccess(data) => {
                        let speed_mb = data.download_speed / 1024.0 / 1024.0;
                        let msg = format!(
                            "[下载] IP {} 测速成功: {:.2} MB/s (Colo: {})",
                            data.ping_data.ip,
                            speed_mb,
                            if data.ping_data.colo.is_empty() {
                                "未知"
                            } else {
                                &data.ping_data.colo
                            }
                        );
                        let _ = dl_tx.send(UiEvent::AddLog(msg));
                    }
                    DownloadEvent::Error { ip, error } => {
                        if is_debug || !matches!(error, NetworkError::Timeout) {
                            let msg = format!("[错误] IP {ip} 下载测速失败: {error}");
                            let _ = dl_tx.send(UiEvent::AddError(msg));
                        }
                    }
                    DownloadEvent::Finished(_) => {}
                }),
            )
            .await
    } else {
        ping_results
    };

    let _ = ui_tx.send(UiEvent::Finished);

    // 显式 drop ui_tx，通知 UI 接收端关闭并结束 UI 循环任务
    drop(ui_tx);

    let (errors, ui_ok) = ui_handle.await.unwrap_or((Vec::new(), false));
    if !ui_ok {
        return;
    }

    // 在普通终端的历史记录中打印结果
    println!("# CF-Picker {VERSION}\n");
    println!("{}", ping_info.bold().cyan());

    for err in &errors {
        println!("{}", err.red());
    }

    if speed_results.is_empty() {
        println!("{}", "[信息] 测速结果 IP 数量为 0。".yellow());
        return;
    }

    if !args.output.trim().is_empty()
        && let Err(e) = export_csv(&args.output, &speed_results)
    {
        eprintln!("{}", format!("[错误] 导出 CSV 写入失败: {e}").red());
    }

    print_results(&speed_results, args.print_num, &args.output);
    end_print(args.print_num);
}

fn print_results(results: &[CloudflareIpData], print_num: usize, output: &str) {
    if print_num == 0 || results.is_empty() {
        return;
    }

    let show_count = min(results.len(), print_num);
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);

    table.set_header(vec![
        Cell::new("IP 地址")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new("已发送")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new("已接收")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new("丢包率")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new("平均延迟 (ms)")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new("下载速度 (MB/s)")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new("地区码")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
    ]);

    for d in results.iter().take(show_count) {
        let rec = d.to_record();
        table.add_row(vec![
            Cell::new(&rec[0]),
            Cell::new(&rec[1]).set_alignment(CellAlignment::Right),
            Cell::new(&rec[2]).set_alignment(CellAlignment::Right),
            Cell::new(&rec[3]).set_alignment(CellAlignment::Right),
            Cell::new(&rec[4]).set_alignment(CellAlignment::Right),
            Cell::new(&rec[5]).set_alignment(CellAlignment::Right),
            Cell::new(&rec[6]).set_alignment(CellAlignment::Center),
        ]);
    }

    println!("{table}");

    if !output.trim().is_empty() {
        println!("\n完整测速结果已写入 {output} 文件，可使用记事本/表格软件查看。");
    }
}

fn end_print(print_num: usize) {
    if print_num == 0 {
        return;
    }
    if cfg!(target_os = "windows") {
        println!("按下 回车键 或 Ctrl+C 退出。");
        let mut buf = String::new();
        let _ = stdin().read_line(&mut buf);
    }
}
