// Adapted from luau-src; licensed under the MIT License in Rust/LICENSE.
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration for building this repository's native Luau sources.
pub struct Build {
    out_dir: Option<PathBuf>,
    target: Option<String>,
    host: Option<String>,
    max_cstack_size: usize,
    use_longjmp: bool,
    enable_codegen: bool,
    vector_size: usize,
}

/// Native archives and linking metadata produced by a build.
pub struct Artifacts {
    lib_dir: PathBuf,
    libs: Vec<String>,
    archives: Vec<PathBuf>,
    link_config: cc::Build,
}

impl Default for Build {
    fn default() -> Self {
        Self {
            out_dir: env::var_os("OUT_DIR").map(PathBuf::from),
            target: env::var("TARGET").ok(),
            host: env::var("HOST").ok(),
            max_cstack_size: 1_000_000,
            use_longjmp: false,
            enable_codegen: false,
            vector_size: 3,
        }
    }
}

impl Build {
    pub fn new() -> Self {
        Self::default()
    }

    /// Overrides Cargo's `OUT_DIR` for the generated archives.
    pub fn out_dir<P: AsRef<Path>>(&mut self, path: P) -> &mut Self {
        self.out_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Overrides Cargo's `TARGET` triple.
    pub fn target(&mut self, target: &str) -> &mut Self {
        self.target = Some(target.to_owned());
        self
    }

    /// Overrides Cargo's `HOST` triple.
    pub fn host(&mut self, host: &str) -> &mut Self {
        self.host = Some(host.to_owned());
        self
    }

    pub fn set_max_cstack_size(&mut self, size: usize) -> &mut Self {
        self.max_cstack_size = size;
        self
    }

    pub fn use_longjmp(&mut self, use_: bool) -> &mut Self {
        self.use_longjmp = use_;
        self
    }

    pub fn enable_codegen(&mut self, enable: bool) -> &mut Self {
        self.enable_codegen = enable;
        self
    }

    pub fn set_vector_size(&mut self, size: usize) -> &mut Self {
        assert!(size == 3 || size == 4, "vector size must be 3 or 4");
        self.vector_size = size;
        self
    }

    pub fn build(&mut self) -> Artifacts {
        let target = self.target.as_deref().expect("TARGET is not set");
        let host = self.host.as_deref().expect("HOST is not set");
        let build_dir = self
            .out_dir
            .as_ref()
            .expect("OUT_DIR is not set")
            .join("luau-build");
        assert!(
            !self.enable_codegen || !target.ends_with("emscripten"),
            "codegen (jit) is not supported on emscripten"
        );

        let source = Path::new(env!("CARGO_MANIFEST_DIR"));
        let common = source.join("Common/include");
        let vm = source.join("VM/include");
        let vm_src = source.join("VM/src");
        let ast = source.join("Ast/include");
        let bytecode = source.join("Bytecode/include");
        let compiler = source.join("Compiler/include");
        let config_include = source.join("Config/include");
        let codegen = source.join("CodeGen/include");
        let inliner = source.join("Inliner/include");
        let require = source.join("Require/include");

        if build_dir.exists() {
            fs::remove_dir_all(&build_dir).expect("failed to clean the Luau build directory");
        }

        let mut config = cc::Build::new();
        config
            .target(target)
            .host(host)
            .out_dir(&build_dir)
            .warnings(false)
            .std("c++17")
            .cpp(true)
            .include(&common)
            .define("LUAI_MAXCSTACK", self.max_cstack_size.to_string().as_str())
            .define("LUA_VECTOR_SIZE", self.vector_size.to_string().as_str())
            .define("LUA_API", "extern \"C\"");

        println!("cargo:rerun-if-env-changed=LUAU_CXXFLAGS");
        println!("cargo:rerun-if-env-changed=LUAU_INCLUDE_DIRS");
        if let Ok(flags) = env::var("LUAU_CXXFLAGS") {
            for flag in flags.split_whitespace() {
                config.flag(flag);
            }
        }
        if let Some(dirs) = env::var_os("LUAU_INCLUDE_DIRS") {
            for dir in env::split_paths(&dirs) {
                config.include(dir);
            }
        }
        if target.ends_with("emscripten") {
            config.flag_if_supported("-fexceptions");
            config.flag_if_supported("-fwasm-exceptions");
        }
        if self.use_longjmp {
            config.define("LUA_USE_LONGJMP", "1");
        }
        if cfg!(debug_assertions) {
            config.define("LUAU_ENABLE_ASSERT", None);
        } else {
            config.flag_if_supported("-fno-math-errno");
        }

        // Dependents precede their dependencies for one-pass static linkers.
        let mut components = vec![
            ("Custom", "luaucustom", vec![&vm, &vm_src], None),
            (
                "Require",
                "luaurequire",
                vec![&require, &ast, &config_include, &vm],
                None,
            ),
            (
                "Config",
                "luauconfig",
                vec![&config_include, &ast, &compiler, &vm, &bytecode],
                None,
            ),
            (
                "Compiler",
                "luaucompiler",
                vec![&compiler, &bytecode, &ast],
                Some("LUACODE_API"),
            ),
            (
                "Inliner",
                "luauinliner",
                vec![&inliner, &bytecode, &vm, &vm_src],
                Some("LUAJITINLINER_API"),
            ),
        ];
        if self.enable_codegen {
            components.push((
                "CodeGen",
                "luaucodegen",
                vec![&codegen, &vm, &vm_src],
                Some("LUACODEGEN_API"),
            ));
        }
        components.extend([
            ("VM", "luauvm", vec![&vm], None),
            ("Bytecode", "luaubytecode", vec![&bytecode], None),
            ("Ast", "luauast", vec![&ast], None),
            ("Common", "luaucommon", vec![], None),
        ]);

        let mut libs = Vec::with_capacity(components.len());
        let mut archives = Vec::with_capacity(components.len());
        for (component, lib, includes, api) in components {
            let component_dir = source.join(component);
            println!("cargo:rerun-if-changed={}", component_dir.display());
            let mut build = config.clone();
            build
                .includes(includes)
                .files(source_files(&component_dir.join("src")));
            if component == "Inliner" {
                build.include(source.join("Bytecode/src"));
            }
            if let Some(api) = api {
                build.define(api, "extern \"C\"");
            }
            let objects = build.compile_intermediates();
            archives.push(build.create_archive(lib, objects));
            libs.push(lib.to_owned());
        }

        Artifacts {
            lib_dir: build_dir,
            libs,
            archives,
            link_config: config,
        }
    }
}

fn source_files(dir: &Path) -> Vec<PathBuf> {
    let mut sources: Vec<_> = fs::read_dir(dir)
        .expect("failed to read the Luau source directory")
        .map(|entry| entry.expect("failed to read a Luau source entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cpp"))
        .collect();
    sources.sort();
    sources
}

impl Artifacts {
    pub fn lib_dir(&self) -> &Path {
        &self.lib_dir
    }

    pub fn libs(&self) -> &[String] {
        &self.libs
    }

    pub fn print_cargo_metadata(&self) {
        for (index, archive) in self.archives.iter().enumerate() {
            let mut config = self.link_config.clone();
            // Emit the effective compiler's runtime after all native archives.
            if index + 1 != self.archives.len() {
                config.cpp_link_stdlib(None);
            }
            cc::emit_link_directives(&config, archive);
        }
        if let Some(version) = self.version() {
            println!("cargo:rustc-env=LUAU_VERSION={version}");
        }
    }

    pub fn version(&self) -> Option<String> {
        let (_, version) = env!("CARGO_PKG_VERSION").split_once("+luau")?;
        Some(format!("0.{version}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_preserves_configuration_contract() {
        for vector in [3, 4] {
            for longjmp in [false, true] {
                for codegen in [false, true] {
                    let mut build = Build::new();
                    build
                        .out_dir("build-output")
                        .target("target")
                        .host("host")
                        .set_max_cstack_size(4096)
                        .set_vector_size(vector)
                        .use_longjmp(longjmp)
                        .enable_codegen(codegen);
                    assert_eq!(build.out_dir.as_deref(), Some(Path::new("build-output")));
                    assert_eq!(build.target.as_deref(), Some("target"));
                    assert_eq!(build.host.as_deref(), Some("host"));
                    assert_eq!(build.max_cstack_size, 4096);
                    assert_eq!(build.vector_size, vector);
                    assert_eq!(build.use_longjmp, longjmp);
                    assert_eq!(build.enable_codegen, codegen);
                }
            }
        }
        let build = Build::default();
        assert_eq!(build.max_cstack_size, 1_000_000);
        assert_eq!(build.vector_size, 3);
        assert!(!build.use_longjmp);
        assert!(!build.enable_codegen);
    }

    #[test]
    fn invalid_vector_widths_are_rejected() {
        for size in [0, 1, 2, 5, usize::MAX] {
            assert!(
                std::panic::catch_unwind(|| Build::new().set_vector_size(size).vector_size)
                    .is_err()
            );
        }
    }

    #[test]
    fn source_discovery_is_sorted_and_uses_the_native_tree() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let files = source_files(&root.join("VM/src"));
        assert!(!files.is_empty());
        assert!(files.contains(&root.join("VM/src/ldebug.cpp")));
        assert!(files.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(files, source_files(&root.join("VM/src")));
    }

    #[test]
    #[should_panic(expected = "TARGET is not set")]
    fn missing_target_is_rejected_before_compilation() {
        Build {
            target: None,
            ..Build::default()
        }
        .build();
    }

    #[test]
    #[should_panic(expected = "HOST is not set")]
    fn missing_host_is_rejected_before_compilation() {
        Build {
            target: Some("target".to_owned()),
            host: None,
            ..Build::default()
        }
        .build();
    }

    #[test]
    #[should_panic(expected = "OUT_DIR is not set")]
    fn missing_output_is_rejected_before_compilation() {
        Build {
            target: Some("target".to_owned()),
            host: Some("host".to_owned()),
            out_dir: None,
            ..Build::default()
        }
        .build();
    }

    #[test]
    fn artifacts_expose_native_version_and_link_inputs() {
        let artifacts = Artifacts {
            lib_dir: PathBuf::from("archives"),
            libs: vec!["luauvm".to_owned()],
            archives: vec![PathBuf::from("archives/libluauvm.a")],
            link_config: cc::Build::new(),
        };
        assert_eq!(artifacts.lib_dir(), Path::new("archives"));
        assert_eq!(artifacts.libs(), &["luauvm"]);
        assert_eq!(artifacts.version().as_deref(), Some("0.742"));
    }
}
