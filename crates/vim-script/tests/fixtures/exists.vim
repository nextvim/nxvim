" Plugin load-guard and namespace dictionary compatibility.
let g:present_value = 1
let s:script_value = 2
let g:compat_result = [exists('g:present_value'), exists('g:missing_value'), exists('s:script_value'), exists('*len'), exists('$PATH'), get(g:, 'missing_value', 7)]
