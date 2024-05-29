use std::collections::BTreeSet;
use std::path::Path;

// UI characters from the Chinese core revision pinned in Cargo.lock.
const CORE_UI_CHARACTERS: &str = concat!(
    "…。「」一下不与丢个习于件位例保修值停入关具内写",
    "出分列创删别到刷前加动助包升单原发取可名否含启器",
    "回在型处外失如存容导将少属工已布帮应建弃当录径微",
    "志快态性意戏手找择持按换捷支改放效文新无日明是显",
    "暂更替最有期未本条析柄标档止正没法活浏消添清游激",
    "父版状用留的盖盘目知示移程空符筛签类级线练组编缺",
    "置能致菜表覆览解触设识说请读路轮载辑输近选配重错",
    "键闭降除隔需项饰！（），：？",
);

fn collect_ui_characters(path: &Path, characters: &mut BTreeSet<char>) {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).expect("Read UI source directory") {
            collect_ui_characters(&entry.expect("Read UI source entry").path(), characters);
        }
    } else if matches!(path.extension().and_then(|ext| ext.to_str()), Some("rs" | "toml")) {
        println!("cargo:rerun-if-changed={}", path.display());
        let source = std::fs::read_to_string(path).expect("Read UI source text");
        characters.extend(source.chars().filter(|c| !c.is_ascii() && !c.is_control()));
    }
}

fn main() {
    embed_resource::compile("./src/resources.rc", embed_resource::NONE);
    println!("cargo:rustc-cdylib-link-arg=/DEF:lib/no-logo/exports.def");

    // Bundle only UI characters, while keeping the runtime font fallback.
    let mut characters = CORE_UI_CHARACTERS.chars().collect::<BTreeSet<_>>();
    for path in ["src", "../jdsd_dsiii_practice_tool.toml", "../lib/libds3/src/catalog"] {
        println!("cargo:rerun-if-changed={path}");
        collect_ui_characters(Path::new(path), &mut characters);
    }
    let output = Path::new(&std::env::var_os("OUT_DIR").expect("Build output directory"))
        .join("ui-glyphs.txt");
    std::fs::write(output, characters.into_iter().collect::<String>())
        .expect("Write UI characters");
}
