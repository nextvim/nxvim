set nocompatible
set nomore
set shortmess+=I
set encoding=utf-8

if v:version != 902 || !has('patch-9.2.843') || has('patch-9.2.844')
  cquit 91
endif

command -nargs=1 -bang SaveAs write<bang> <args>
call setline(1, 'phase-d')
execute 'SaveAs! ' . fnameescape($NXVIM_PHASE_D_TARGET)
set ff=dos fenc=utf-8 ro bin noeol nofixeol noma
call writefile([
      \ printf('%d,%d,%d,%d,%d,%s,%s',
      \   &modifiable,
      \   &readonly,
      \   &binary,
      \   &endofline,
      \   &fixeol,
      \   &fileformat,
      \   &fileencoding)
      \ ], $NXVIM_ORACLE_OUTPUT)
qa!
