" Yes/No Confirmation Popup script for NxVim
" Usage: :source data/tests/vim/popup_input.vim
" Press 'y' or 'n' or Esc to dismiss

function! MyInputFilter(winid, key)
    call popup_settext(a:winid, ['Confirm action: ' . a:key, '(y/n)'])
    return popup_filter_yesno(a:winid, a:key)
endfunction

let g:popup_id = popup_create(['Confirm action?', '(y/n)'], { 'line': 5, 'col': 15, 'title': ' Prompt ', 'border': [1, 1, 1, 1], 'filter': 'MyInputFilter' })
