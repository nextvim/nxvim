set nocompatible
set nomore
set shortmess+=I
set encoding=utf-8

if v:version != 902 || !has('patch-9.2.843') || has('patch-9.2.844')
  cquit 91
endif

autocmd TextChanged * ++once undo
call setline(1, 'first')
doautocmd TextChanged
call setline(1, 'second')
doautocmd TextChanged
call writefile([getline(1)], $NXVIM_ORACLE_OUTPUT)
qa!
