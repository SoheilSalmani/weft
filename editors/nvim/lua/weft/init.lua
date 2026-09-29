-- weft.nvim — thin Neovim integration for weft templates.
--
-- * LSP: registers `weft lsp` for weft.toml and patches/*.json buffers.
-- * :WeftNew [template] [dest] — runs `weft new` in a floating terminal, where
--   weft asks the template's questions (nothing bespoke here).
--
-- Setup:
--   require("weft").setup({ bin = "weft" })

local M = {}

local config = { bin = "weft" }

local function template_root(bufname)
  local dir = vim.fs.dirname(bufname)
  local found = vim.fs.find("weft.toml", { upward = true, path = dir })[1]
  return found and vim.fs.dirname(found) or nil
end

local function start_lsp(bufnr)
  local root = template_root(vim.api.nvim_buf_get_name(bufnr))
  if not root then
    return
  end
  vim.lsp.start({
    name = "weft",
    cmd = { config.bin, "lsp" },
    root_dir = root,
  }, { bufnr = bufnr })
  -- rendered-value previews next to {"answer": ...} / {"expr": ...} segments
  if vim.lsp.inlay_hint then
    vim.lsp.inlay_hint.enable(true, { bufnr = bufnr })
  end
end

function M.setup(opts)
  config = vim.tbl_extend("force", config, opts or {})

  vim.api.nvim_create_autocmd({ "BufReadPost", "BufNewFile" }, {
    pattern = { "weft.toml", "*/patches/*.json" },
    callback = function(event)
      start_lsp(event.buf)
    end,
  })

  vim.api.nvim_create_user_command("WeftNew", function(cmd)
    local args = vim.split(cmd.args, "%s+", { trimempty = true })
    M.new(args[1], args[2])
  end, { nargs = "*", complete = "dir", desc = "Scaffold a project from a weft template" })
end

--- Run `weft new` in a floating terminal.
---@param template string|nil template directory (prompted when nil)
---@param dest string|nil destination directory (prompted when nil)
function M.new(template, dest)
  local function ask(prompt, default, next_step)
    vim.ui.input({ prompt = prompt, default = default, completion = "dir" }, function(value)
      if value and value ~= "" then
        next_step(value)
      end
    end)
  end

  local function launch(tpl, target)
    local width = math.min(100, math.floor(vim.o.columns * 0.9))
    local height = math.min(30, math.floor(vim.o.lines * 0.85))
    local buf = vim.api.nvim_create_buf(false, true)
    local win = vim.api.nvim_open_win(buf, true, {
      relative = "editor",
      width = width,
      height = height,
      col = math.floor((vim.o.columns - width) / 2),
      row = math.floor((vim.o.lines - height) / 2),
      style = "minimal",
      border = "rounded",
      title = " weft new ",
    })
    vim.fn.jobstart({ config.bin, "new", tpl, target }, {
      term = true,
      on_exit = function(_, code)
        if vim.api.nvim_win_is_valid(win) then
          vim.api.nvim_win_close(win, true)
        end
        if code == 0 then
          vim.notify("weft: scaffolded " .. target)
        else
          vim.notify("weft new exited with " .. code, vim.log.levels.WARN)
        end
      end,
    })
    vim.cmd.startinsert()
  end

  if template and dest then
    launch(template, dest)
  elseif template then
    ask("Destination directory: ", "", function(d)
      launch(template, d)
    end)
  else
    ask("Template directory: ", vim.fn.getcwd(), function(t)
      ask("Destination directory: ", "", function(d)
        launch(t, d)
      end)
    end)
  end
end

return M
