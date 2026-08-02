" A second script proves that s: state does not collide across files.
let s:private_value = 22
let g:second_private_value = s:private_value
let g:cross_module_value = g:BasicPluginValue()
let g:autoload_value = demo#util#answer()
:BasicSet enabled
