" Hello World popup script for NxVim
" Usage: :source data/tests/vim/popup.vim

let g:popup_id = popup_create('Hello World', { 'line': 5, 'col': 15, 'title': ' Greetings ', 'border': [1, 1, 1, 1], 'time': 3000 })
