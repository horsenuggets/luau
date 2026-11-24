#[cfg(test)]
mod tests {
    use mlua::chunk::Compiler;
    use mlua::function::CoverageInfo;
    use mlua::{Function, Lua, Result, Table, Value};

    fn coverage(function: &Function) -> Vec<CoverageInfo> {
        let mut entries = Vec::new();
        function.coverage(|entry| entries.push(entry));
        entries
    }

    fn check_coverage(entries: &[CoverageInfo], level: u8, root_hits: i32, child_hits: i32) {
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].function, None);
        assert_eq!(entries[0].line_defined, 1);
        assert_eq!(entries[0].depth, 0);
        assert_eq!(entries[1].function.as_deref(), Some("makeTable"));
        assert_eq!(entries[1].line_defined, 1);
        assert_eq!(entries[1].depth, 1);
        assert!(entries[0].hits.len() > 705);
        assert_eq!(entries[0].hits.len(), entries[1].hits.len());
        for (line, (&root, &child)) in entries[0].hits.iter().zip(&entries[1].hits).enumerate() {
            assert_eq!(
                root,
                if line == 1 || line == 705 {
                    root_hits
                } else {
                    -1
                },
                "root line {line}"
            );
            let executable = line == 2 || (level == 2 && (3..=702).contains(&line));
            assert_eq!(
                child,
                if executable { child_hits } else { -1 },
                "child line {line}"
            );
        }
    }

    #[test]
    fn coverage_preserves_exact_native_entries() -> Result<()> {
        for level in [1, 2] {
            let lua = Lua::new();
            let mut source = String::from("local function makeTable()\nreturn {\n");
            for field in 1..=700 {
                source.push_str(&format!("[\"Key{field}\"] = true,\n"));
            }
            source.push_str("}\nend\nreturn makeTable\n");
            let compiler = Compiler::new()
                .set_optimization_level(1)
                .set_coverage_level(level);
            let chunk = lua.load(source).set_compiler(compiler).into_function()?;
            check_coverage(&coverage(&chunk), level, 0, 0);
            let child: Function = chunk.call(())?;
            check_coverage(&coverage(&chunk), level, 1, 0);
            for hits in 1..=2 {
                let table: Table = child.call(())?;
                assert!(table.get::<bool>("Key700")?);
                check_coverage(&coverage(&chunk), level, 1, hits);
            }
        }
        Ok(())
    }

    #[test]
    fn runtime_callbacks_coroutines_and_gc_match_the_ffi_contract() -> Result<()> {
        assert_eq!(mlua::ffi::luau_version(), Some("0.742"));
        assert_eq!(mlua::ffi::LUA_REGISTRYINDEX, -1_002_000);
        let lua = Lua::new();
        lua.globals().set(
            "double",
            lua.create_function(|_, value: i64| Ok(value * 2))?,
        )?;
        lua.globals().set(
            "fail",
            lua.create_function(|_, ()| -> Result<()> {
                Err(mlua::Error::runtime("expected callback failure"))
            })?,
        )?;
        lua.load(
            r#"
            for i = 1, 100 do
                assert(double(i) == i * 2)
                local ok, message = pcall(fail)
                assert(not ok and string.find(tostring(message), "expected callback failure"))
            end
            local mt = { __index = { value = 42 } }
            assert(setmetatable({}, mt).value == 42)
        "#,
        )
        .exec()?;
        let function: Function = lua
            .load("return function(n) coroutine.yield(n + 1); return n + 2 end")
            .eval()?;
        let thread = lua.create_thread(function)?;
        assert_eq!(thread.resume::<i64>(40)?, 41);
        assert_eq!(thread.resume::<i64>(())?, 42);
        lua.gc_collect()?;
        lua.gc_collect()?;
        assert_eq!(lua.load("return double(21)").eval::<i64>()?, 42);
        Ok(())
    }

    #[test]
    fn vectors_match_the_selected_native_width() -> Result<()> {
        let lua = Lua::new();
        let value: Value = lua.load("return vector.create(1, 2, 3, 4)").eval()?;
        let Value::Vector(vector) = value else {
            panic!("expected a native vector")
        };
        #[cfg(feature = "vector4")]
        assert_eq!(vector, mlua::Vector::new(1.0, 2.0, 3.0, 4.0));
        #[cfg(not(feature = "vector4"))]
        assert_eq!(vector, mlua::Vector::new(1.0, 2.0, 3.0));
        Ok(())
    }

    #[cfg(feature = "codegen")]
    #[test]
    fn codegen_executes_through_the_real_mlua_consumer() -> Result<()> {
        let supported = unsafe { mlua::ffi::luau_codegen_supported() } != 0;
        if cfg!(feature = "vector4") || !cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
        {
            assert!(!supported);
        } else if cfg!(target_arch = "aarch64") {
            assert!(supported);
        }
        let lua = Lua::new();
        lua.enable_jit(true);
        lua.globals().set(
            "hasNativeCode",
            lua.create_function(|lua, ()| unsafe {
                lua.exec_raw::<bool>((), |state| {
                    let native = (0..mlua::ffi::lua_stackdepth(state))
                        .any(|level| mlua::ffi::lua_hascustomexecution(state, level) != 0);
                    mlua::ffi::lua_pushboolean(state, i32::from(native));
                })
            })?,
        )?;
        let function = lua
            .load("local native = hasNativeCode(); local total = 0; for i = 1, 10000 do total += i end; return total, native")
            .into_function()?;
        assert_eq!(function.call::<(i64, bool)>(())?, (50_005_000, supported));
        Ok(())
    }
}
