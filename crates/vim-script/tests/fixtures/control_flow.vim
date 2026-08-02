" Derived from Vim test_vimscript.vim loop and branch cases.
let total = 0
for item in range(1, 4)
  if item % 2 == 0
    let total = total + item
  else
    let total = total + 10
  endif
endfor
let counter = 0
while counter < 3
  let counter = counter + 1
endwhile
let g:compat_result = [total, counter]
