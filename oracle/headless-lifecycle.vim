set nocompatible
set nomore
set shortmess+=I
set encoding=utf-8

if v:version != 902 || !has('patch-9.2.843') || has('patch-9.2.844')
  cquit 91
endif

let g:first = bufnr('%')
enew
let g:after_pristine_enew = bufnr('%')
call setline(1, 'changed')
enew!
let g:second = bufnr('%')

call writefile([
      \ printf('%d,%d,%d,%d,%d,%d,%d',
      \   g:first,
      \   g:after_pristine_enew,
      \   g:second,
      \   bufnr('#'),
      \   bufexists(g:first),
      \   bufloaded(g:first),
      \   buflisted(g:first))
      \ ], $NXVIM_ORACLE_OUTPUT)

execute 'bwipeout! ' . g:first
call writefile([
      \ printf('%d,%d,%d', bufnr('%'), bufexists(g:first), buflisted(bufnr('%')))
      \ ], $NXVIM_ORACLE_OUTPUT, 'a')
qa!
