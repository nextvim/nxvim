" Deterministic Vim regex oracle. Input/output paths are provided by Rust.
set nocompatible
set nomore
set shortmess+=I
set encoding=utf-8
set fileencoding=utf-8
set regexpengine=0
set magic
set noignorecase
set nosmartcase
set iskeyword=@,48-57,_,192-255
set isfname=@,48-57,/,.,-,_,+,,,#,$,%,~,=
set isprint=@,161-255

function! s:write_result(result) abort
  let l:path = getenv('VIM_REGEX_ORACLE_OUTPUT')
  if empty(l:path)
    cquit 90
  endif
  call writefile([json_encode(a:result)], l:path, 'b')
endfunction

function! s:byte_position(text, offset) abort
  if a:offset < 0 || a:offset > strlen(a:text)
    return v:null
  endif
  let l:prefix = strpart(a:text, 0, a:offset)
  let l:lines = split(l:prefix, "\n", 1)
  return [len(l:lines), strlen(l:lines[-1]) + 1]
endfunction

function! s:set_optional_options(options) abort
  if has_key(a:options, 'magic') && a:options.magic isnot v:null
    let &magic = a:options.magic
  endif
  if has_key(a:options, 'ignore_case') && a:options.ignore_case isnot v:null
    let &ignorecase = a:options.ignore_case
  endif
  if has_key(a:options, 'smart_case') && a:options.smart_case isnot v:null
    let &smartcase = a:options.smart_case
  endif
  if has_key(a:options, 'is_keyword') && a:options.is_keyword isnot v:null
    let &l:iskeyword = a:options.is_keyword
  endif
  if has_key(a:options, 'is_file_name') && a:options.is_file_name isnot v:null
    let &isfname = a:options.is_file_name
  endif
  if has_key(a:options, 'is_print') && a:options.is_print isnot v:null
    let &isprint = a:options.is_print
  endif
endfunction

try
  if v:version != 902 || !has('patch-9.2.843') || has('patch-9.2.844')
    call s:write_result({
          \ 'status': 'incompatible_vim',
          \ 'vim_version': v:version,
          \ 'required_patch': 843,
          \ 'message': 'oracle requires Vim 9.2 with patches 1-843'
          \ })
    cquit 91
  endif

  let s:input_path = getenv('VIM_REGEX_ORACLE_INPUT')
  if empty(s:input_path)
    throw 'oracle:missing-input-path'
  endif
  let s:request = json_decode(join(readfile(s:input_path, 'b'), "\n"))
  call s:set_optional_options(get(s:request, 'options', {}))

  let s:features = get(s:request, 'features', [])
  if index(s:features, 'visual-area') >= 0
    call s:write_result({
          \ 'status': 'unsupported',
          \ 'vim_version': v:version,
          \ 'fixture_id': s:request.id,
          \ 'reason': 'visual selection modes require a dedicated buffer oracle'
          \ })
    qa!
  endif
  if index(s:features, 'cursor') >= 0 || index(s:features, 'line') >= 0
    call s:write_result({
          \ 'status': 'unsupported',
          \ 'vim_version': v:version,
          \ 'fixture_id': s:request.id,
          \ 'reason': 'line and cursor atoms require a dedicated buffer-search oracle'
          \ })
    qa!
  endif
  if index(s:features, 'smartcase') >= 0
    call s:write_result({
          \ 'status': 'unsupported',
          \ 'vim_version': v:version,
          \ 'fixture_id': s:request.id,
          \ 'reason': 'matchstrpos() does not apply the smartcase search heuristic'
          \ })
    qa!
  endif

  silent %delete _
  call setline(1, split(s:request.input, "\n", 1))
  let &l:tabstop = get(get(s:request, 'editor', {}), 'tab_stop', 8)

  let s:editor = get(s:request, 'editor', {})
  if has_key(s:editor, 'cursor') && s:editor.cursor isnot v:null
    let s:cursor_position = s:byte_position(s:request.input, s:editor.cursor)
    if s:cursor_position is v:null
      throw 'oracle:invalid-cursor-offset'
    endif
    call cursor(s:cursor_position[0], s:cursor_position[1])
  endif

  if has_key(s:editor, 'visual') && s:editor.visual isnot v:null
    let s:visual_start = s:byte_position(s:request.input, s:editor.visual.range.start)
    let s:visual_end = s:byte_position(s:request.input, s:editor.visual.range.end)
    if s:visual_start is v:null || s:visual_end is v:null
      throw 'oracle:invalid-visual-range'
    endif
    call setpos("'<", [0, s:visual_start[0], s:visual_start[1], 0])
    call setpos("'>", [0, s:visual_end[0], s:visual_end[1], 0])
  endif

  let s:position = matchstrpos(s:request.input, s:request.pattern)
  if s:position[1] < 0
    call s:write_result({
          \ 'status': 'no_match',
          \ 'vim_version': v:version,
          \ 'fixture_id': s:request.id
          \ })
  else
    let s:captures = matchlist(s:request.input, s:request.pattern)
    call s:write_result({
          \ 'status': 'match',
          \ 'vim_version': v:version,
          \ 'fixture_id': s:request.id,
          \ 'range': {'start': s:position[1], 'end': s:position[2]},
          \ 'capture_texts': s:captures
          \ })
  endif
catch /^Vim\%((\a\+)\)\=:E\d\+/
  call s:write_result({
        \ 'status': 'diagnostic',
        \ 'vim_version': v:version,
        \ 'fixture_id': get(get(s:, 'request', {}), 'id', ''),
        \ 'code': matchstr(v:exception, 'E\d\+')
        \ })
catch
  call s:write_result({
        \ 'status': 'protocol_error',
        \ 'vim_version': v:version,
        \ 'fixture_id': get(get(s:, 'request', {}), 'id', ''),
        \ 'code': substitute(v:exception, '^oracle:', '', '')
        \ })
  cquit 92
endtry

qa!
