fn main() {
    // tauri-build 在编译期生成 context（图标、能力清单、嵌入的前端资源）。
    // 失败时 panic 带的是「哪个图标/清单项不对」，比链接期报错可读。
    tauri_build::build()
}
