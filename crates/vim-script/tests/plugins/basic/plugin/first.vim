" Tier-1 plugin fixture: load guard, global initialization, script-local state.
if exists('g:basic_plugin_loaded')
  finish
endif
let g:basic_plugin_loaded = 1
let g:guard_default = get(g:, 'missing_value', 7)
let s:private_value = 11
let g:first_private_value = s:private_value
function g:BasicPluginValue()
  return s:private_value
endfunction
:command! -nargs=1 BasicSet set plugin_value=<args>
:nnoremap <silent> <leader>w :BasicSet mapped<CR>
:augroup BasicPlugin
:autocmd!
:autocmd BufEnter *.txt ++once BasicSet event
:augroup END
:write
