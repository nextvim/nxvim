" Derived from Vim test_functions.vim argument, return, and local-scope cases.
function CompatAdd(left, right)
  let local = a:left + a:right
  return local
endfunction
let g:compat_result = [CompatAdd(20, 22), CompatAdd(-2, 5)]
