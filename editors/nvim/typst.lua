-- ftplugin/typst.lua
--
-- This setup detects a main.typst file and compiles it

local root_files = { 'main.typst' }
local paths = vim.fs.find(root_files, { stop = vim.env.HOME })
local root_dir = vim.fs.dirname(paths[1])

if root_dir then
  vim.lsp.start({
    cmd = { 'typst-languagetool-lsp' },
    filetype = { 'typst' },
    root_dir = root_dir,
    init_options = {
      backend = "bundle", -- "bundle" | "jar" | "server"
      -- jar_location = "path/to/jar/location"
      -- host = "http://127.0.0.1",
      -- port = "8081",
      root = root_dir,
      main = root_dir .. "/main.typst",
      languages = { de = "de-DE", en = "en-US" }
    },
  })
end
