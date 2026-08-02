" Derived from Vim test_trycatch.vim catch/finally behavior.
let value = 0
try
  throw 'failure'
catch
  let value = 10
finally
  let value = value + 1
endtry
let g:compat_result = value
