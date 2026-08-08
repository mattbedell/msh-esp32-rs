-- NOTE: Manually change targeted chip variant here so LS works
-- NOTE: Enable nvim exrc and :trust
local rust_analyzer = {
    cargo = {
        target = "xtensa-esp32s3-none-elf",
        allTargets = false,
    },
    check = {
      targets = { "xtensa-esp32s3-none-elf" },
    },
    rustcSource = "discover",
}
rust_analyzer.cargo.extraEnv = { RUST_TOOLCHAIN = "esp", CARGO_TOOLCHAIN = "esp", RUSTUP_TOOLCHAIN = "esp" }
-- rust_analyzer.server = { extraEnv = { RUST_TOOLCHAIN = "stable" } }

-- Note the neovim name of the language server is rust_analyzer with an underscore.
vim.lsp.config("rust_analyzer", {
    settings = {
        ["rust-analyzer"] = rust_analyzer,
    },
})
