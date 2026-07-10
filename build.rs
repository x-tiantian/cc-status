// build.rs — 把应用图标嵌入 exe 资源段。
// build.rs 的工作目录是包根目录,故路径为 resources/icon.ico。
// winres 会把该图标作为可执行文件的默认应用图标(资源 ID 1),
// 资源管理器/任务栏/Alt-Tab 均使用它;托盘图标运行时用 LoadImageW(MAKEINTRESOURCE(1)) 读取。

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon_with_id("resources/icon.ico", "1");
        if let Err(e) = res.compile() {
            // 图标嵌入失败不应阻断编译(如缺少 rc 工具);仅告警。
            println!("cargo:warning=图标嵌入失败: {e}");
        }
    }
    println!("cargo:rerun-if-changed=resources/icon.ico");
}
