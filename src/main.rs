use std::io::{stdout, Write};
use std::{thread, time::Duration};
use crossterm::{
    ExecutableCommand,
    cursor::{Hide, MoveTo, Show},
    style::{Color, Stylize},
    terminal::{Clear, ClearType, size},
};

fn main() {
    let mut stdout = stdout();
    stdout.execute(Hide).unwrap(); // 隐藏光标

    // 更平滑的颜色渐变：蓝 → 青 → 绿
    let gradient_steps: Vec<(u8, u8, u8)> = (0..=50)
        .step_by(2)
        .map(|g| {
            let g = g as u8;
            let r = 0;
            let green = g * 5;
            let blue = 255u8.saturating_sub(g * 5);
            (r, green, blue)
        })
        .collect();

    // 使用空心圆作为最后的 spinner 符号
    let spinner = ["◜", "◠", "◝", "◞", "◡", "◟", "○"]; // 空心圆符号

    let total_steps = gradient_steps.len().saturating_sub(16); // 色块数量为16个
    let total_steps = total_steps.min(gradient_steps.len() - 1);

    // 获取终端宽度（只用 _cols）
    let (_cols, _) = size().unwrap_or((80, 24)); // 默认宽度为80，行数为24

    // 控制输出，避免中间的空行
    let total_loops = 3; // 控制色条循环的次数

    // 外层循环6次
    for loop_count in 0..total_loops {
        for i in 0..total_steps {
            let mut block_str = String::new();

            // 构造16个渐变色块
            for j in 0..16 {
                if i + j < gradient_steps.len() {
                    let (r, g, b) = gradient_steps[i + j];
                    block_str.push_str(&format!("{}", "█".with(Color::Rgb { r, g, b })));
                }
            }

            // 计算百分比：基于外层循环的进度
            let percent = (((loop_count * total_steps) + i) * 100) / (total_loops * total_steps); // 计算整个进度条的百分比
            let spin = spinner[i % spinner.len()];
            let info = format!("{} {}%", spin.with(Color::Green), percent);

            let full_line = format!("{} {}", block_str, info);

            // 在每次更新时输出进度条
            stdout.execute(MoveTo(0, 0)).unwrap();
            stdout.execute(Clear(ClearType::CurrentLine)).unwrap();
            print!("{}", full_line);
            stdout.flush().unwrap();  // 将缓冲区内容写入屏幕
            thread::sleep(Duration::from_millis(100)); // 控制刷新速度
        }
    }

    // 覆盖输出 "Start" 在行首
    let final_msg = "Hermes Start!               ";

    // 定位到行首并覆盖输出 "Start"
    stdout.execute(MoveTo(0, 0)).unwrap();
    print!("{}", final_msg.bold().with(Color::Cyan));
    stdout.flush().unwrap();

    stdout.execute(Show).unwrap(); // 恢复光标
    thread::sleep(Duration::from_secs(2)); // 等待2秒以便用户看到最终结果
}


















