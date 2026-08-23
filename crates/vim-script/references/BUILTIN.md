# Builtin Functions Status

This document tracks the implementation status of all Vim Script builtin functions from `builtin.txt`.

## How to Implement a Builtin Function

Follow these steps to implement a new builtin function:

1. **Implement the Function Logic**:
   - Locate the target submodule under [`crates/vim-script/src/runtime/builtins/`](file:///home/iceman/Developer/rust/nextvim/nxvim/crates/vim-script/src/runtime/builtins/):
     - `math.rs` for mathematical operations.
     - `string.rs` for string and conversion utilities.
     - `collections.rs` for list, dict, blob, tuple operations.
     - `state.rs` for buffer context, options, namespaces.
     - `types.rs` for value type inspections.
   - Write your implementation function (e.g., `pub fn my_func(args: &[Value]) -> RuntimeResult<Value>`).
   - Use helper functions like `type_error` or `error("EXXX", "message")` to handle validation errors.

2. **Register the Function**:
   - Register it inside the `BuiltinRegistry::with_defaults()` function in [`crates/vim-script/src/runtime/builtins/mod.rs`](file:///home/iceman/Developer/rust/nextvim/nxvim/crates/vim-script/src/runtime/builtins/mod.rs#L44):
     ```rust
     registry.register("func_name", BuiltinArity::Exact(1), submodule::func_name);
     ```
   - Select the correct `BuiltinArity` variant:
     - `BuiltinArity::Exact(n)`
     - `BuiltinArity::Range { min: a, max: b }`
     - `BuiltinArity::Variadic { min: a }`

3. **Add Compatibility Tests**:
   - Open the test fixture file [`crates/vim-script/tests/fixtures/builtins.vim`](file:///home/iceman/Developer/rust/nextvim/nxvim/crates/vim-script/tests/fixtures/builtins.vim).
   - Add a call to the new function and append the result to the `g:compat_result` list.
   - Update the expected string snapshot in [`crates/vim-script/tests/compatibility.rs`](file:///home/iceman/Developer/rust/nextvim/nxvim/crates/vim-script/tests/compatibility.rs#L22) (under `"builtins.vim"` in `COMPATIBILITY_FIXTURES`).

4. **Verify the Tests**:
   - Run the tests via:
     ```bash
     cargo test -p vim-script
     ```

5. **Update This Tracking File**:
   - Check off the implemented function in the list below by changing `- [ ]` to `- [x]`.

Total functions: 598

## Math / Bitwise

- [x] `abs({expr})` -> Float/Number - absolute value of {expr}
- [x] `acos({expr})` -> Float - arc cosine of {expr}
- [x] `and({expr}, {expr})` -> Number - bitwise AND
- [x] `asin({expr})` -> Float - arc sine of {expr}
- [x] `atan({expr})` -> Float - arc tangent of {expr}
- [x] `atan2({expr1}, {expr2})` -> Float - arc tangent of {expr1} / {expr2}
- [x] `ceil({expr})` -> Float - round {expr} up
- [x] `cos({expr})` -> Float - cosine of {expr}
- [x] `cosh({expr})` -> Float - hyperbolic cosine of {expr}
- [x] `exp({expr})` -> Float - exponential of {expr}
- [x] `float2nr({expr})` -> Number - convert Float {expr} to a Number
- [x] `floor({expr})` -> Float - round {expr} down
- [x] `fmod({expr1}, {expr2})` -> Float - remainder of {expr1} / {expr2}
- [x] `invert({expr})` -> Number - bitwise invert
- [x] `isinf({expr})` -> Number - determine if {expr} is infinity value (positive or negative)
- [x] `isnan({expr})` -> Number - |TRUE| if {expr} is NaN
- [x] `log({expr})` -> Float - natural logarithm (base e) of {expr}
- [x] `log10({expr})` -> Float - logarithm of Float {expr} to base 10
- [x] `or({expr}, {expr})` -> Number - bitwise OR
- [x] `pow({x}, {y})` -> Float - {x} to the power of {y}
- [x] `rand([{expr}])` -> Number - get pseudo-random number
- [x] `round({expr})` -> Float - round off {expr}
- [x] `sin({expr})` -> Float - sine of {expr}
- [x] `sinh({expr})` -> Float - hyperbolic sine of {expr}
- [x] `sqrt({expr})` -> Float - square root of {expr}
- [x] `srand([{expr}])` -> List - get seed for |rand()|
- [x] `tan({expr})` -> Float - tangent of {expr}
- [x] `tanh({expr})` -> Float - hyperbolic tangent of {expr}
- [x] `trunc({expr})` -> Float - truncate Float {expr}
- [x] `xor({expr}, {expr})` -> Number - bitwise XOR

## String Manipulation

- [x] `base64_decode({string})` -> Blob - base64 decode {string} characters
- [x] `base64_encode({blob})` -> String - base64 encode the bytes in {blob}
- [x] `char2nr({expr} [, {utf8}])` -> Number - ASCII/UTF-8 value of first char in {expr}
- [ ] `charclass({string})` -> Number - character class of {string}
- [ ] `charidx({string}, {idx} [, {countcc} [, {utf16}]])` -> Number - char index of byte {idx} in {string}
- [x] `escape({string}, {chars})` -> String - escape {chars} in {string} with '\'
- [x] `fnameescape({fname})` -> String - escape special characters in {fname}
- [ ] `iconv({expr}, {from}, {to})` -> String - convert encoding of {expr}
- [x] `join({expr} [, {sep}])` -> String - join items in {expr} into one String
- [ ] `keytrans({string})` -> String - translate internal keycodes to a form that can be used by `:map`
- [x] `nr2char({expr} [, {utf8}])` -> String - single char with ASCII/UTF-8 value {expr}
- [x] `printf({fmt}, {expr1}...)` -> String - format text
- [ ] `soundfold({word})` -> String - sound-fold {word}
- [x] `split({expr} [, {pat} [, {keepempty}]])` -> List - make |List| from {pat} separated {expr}
- [x] `str2float({expr} [, {quoted}])` -> Float - convert String to Float
- [x] `str2list({expr} [, {utf8}])` -> List - convert each character of {expr} to ASCII/UTF-8 value
- [x] `str2nr({expr} [, {base} [, {quoted}]])` -> Number - convert String to Number
- [x] `strcharlen({expr})` -> Number - character length of the String {expr}
- [x] `strcharpart({str}, {start} [, {len} [, {skipcc}]])` -> String - {len} characters of {str} at character {start}
- [x] `strchars({expr} [, {skipcc}])` -> Number - character count of the String {expr}
- [ ] `strdisplaywidth({expr} [, {col}])` -> Number - display length of the String {expr}
- [x] `strgetchar({str}, {index})` -> Number - get char {index} from {str}
- [x] `stridx({haystack}, {needle} [, {start}])` -> Number - index of {needle} in {haystack}
- [x] `strlen({expr})` -> Number - length of the String {expr}
- [x] `strpart({str}, {start} [, {len} [, {chars}]])` -> String - {len} bytes/chars of {str} at byte {start}
- [x] `strridx({haystack}, {needle} [, {start}])` -> Number - last index of {needle} in {haystack}
- [ ] `strtrans({expr})` -> String - translate string to make it printable
- [ ] `strutf16len({string} [, {countcc}])` -> Number - number of UTF-16 code units in {string}
- [x] `strwidth({expr})` -> Number - display cell length of the String {expr}
- [ ] `submatch({nr} [, {list}])` -> String/List - specific match in `:substitute` or substitute()
- [ ] `substitute({expr}, {pat}, {sub}, {flags})` -> String - all {pat} in {expr} replaced with {sub}
- [x] `tolower({expr})` -> String - the String {expr} switched to lowercase
- [x] `toupper({expr})` -> String - the String {expr} switched to uppercase
- [x] `tr({src}, {fromstr}, {tostr})` -> String - translate chars of {src} in {fromstr} to chars in {tostr}
- [x] `trim({text} [, {mask} [, {dir}]])` -> String - trim characters in {mask} from {text}
- [x] `uri_decode({string})` -> String - URI-decode a string
- [x] `uri_encode({string})` -> String - URI-encode a string
- [ ] `utf16idx({string}, {idx} [, {countcc} [, {charidx}]])` -> Number - UTF-16 index of byte {idx} in {string}

## Buffer Mutation

- [ ] `append({lnum}, {text})` -> Number - append {text} below line {lnum}
- [ ] `appendbufline({buf}, {lnum}, {text})` -> Number - append {text} below line {lnum} in buffer {buf}
- [ ] `deletebufline({buf}, {first} [, {last}])` -> Number - delete lines from buffer {buf}
- [ ] `setbufline({buf}, {lnum}, {text})` -> Number - set line {lnum} to {text} in buffer {buf}
- [ ] `setbufvar({buf}, {varname}, {val})` -> none - set {varname} in buffer {buf} to {val}
- [ ] `setline({lnum}, {line})` -> Number - set line {lnum} to {line}

## Buffer Query

- [ ] `bufadd({name})` -> Number - add a buffer to the buffer list
- [ ] `bufexists({buf})` -> Number - |TRUE| if buffer {buf} exists
- [ ] `buflisted({buf})` -> Number - |TRUE| if buffer {buf} is listed
- [ ] `bufload({buf})` -> none - load buffer {buf} if not loaded yet
- [ ] `bufloaded({buf})` -> Number - |TRUE| if buffer {buf} is loaded
- [ ] `bufname([{buf}])` -> String - name of the buffer {buf}
- [ ] `bufnr([{buf} [, {create}]])` -> Number - number of the buffer {buf}
- [ ] `bufwinid({buf})` -> Number - window ID of buffer {buf}
- [ ] `bufwinnr({buf})` -> Number - window number of buffer {buf}
- [ ] `byte2line({byte})` -> Number - line number at byte count {byte}
- [ ] `getbufinfo([{buf}])` -> List - information about buffers
- [ ] `getbufline({buf}, {lnum} [, {end}])` -> List - lines {lnum} to {end} of buffer {buf}
- [ ] `getbufoneline({buf}, {lnum})` -> String - line {lnum} of buffer {buf}
- [ ] `getbufvar({buf}, {varname} [, {def}])` -> any - variable {varname} in buffer {buf}
- [ ] `getline({lnum})` -> String - line {lnum} of current buffer
- [ ] `getline({lnum}, {end})` -> List - lines {lnum} to {end} of current buffer
- [ ] `line({expr} [, {winid}])` -> Number - line nr of cursor, last line or mark
- [ ] `line2byte({lnum})` -> Number - byte count of line {lnum}

## Collections (List, Dict, Blob, Tuple)

- [x] `add({object}, {item})` -> List/Blob - append {item} to {object}
- [ ] `blob2list({blob})` -> List - convert {blob} into a list of numbers
- [ ] `blob2str({blob} [, {options}])` -> List - convert {blob} into a list of strings
- [ ] `copy({expr})` -> any - make a shallow copy of {expr}
- [ ] `count({comp}, {expr} [, {ic} [, {start}]])` -> Number - count how many {expr} are in {comp}
- [ ] `deepcopy({expr} [, {noref}])` -> any - make a full copy of {expr}
- [x] `empty({expr})` -> Number - |TRUE| if {expr} is empty
- [ ] `extend({expr1}, {expr2} [, {expr3}])` -> List/Dict - insert items of {expr2} into {expr1}
- [ ] `extendnew({expr1}, {expr2} [, {expr3}])` -> List/Dict - like |extend()| but creates a new List or Dictionary
- [ ] `filter({expr1}, {expr2})` -> List/Dict/Blob/String - remove items from {expr1} where {expr2} is 0
- [ ] `flatten({list} [, {maxdepth}])` -> List - flatten {list} up to {maxdepth} levels
- [ ] `flattennew({list} [, {maxdepth}])` -> List - flatten a copy of {list}
- [ ] `foreach({expr1}, {expr2})` -> List/Tuple/Dict/Blob/String - for each item in {expr1} call {expr2}
- [x] `get({dict}, {key} [, {def}])` -> any - get item {key} from {dict} or {def}
- [x] `get({func}, {what})` -> any - get property of funcref/partial {func}
- [x] `get({list}, {idx} [, {def}])` -> any - get item {idx} from {list} or {def}
- [ ] `has_key({dict}, {key})` -> Number - |TRUE| if {dict} has entry {key}
- [ ] `index({object}, {expr} [, {start} [, {ic}]])` -> Number - index in {object} where {expr} appears
- [ ] `indexof({object}, {expr} [, {opts}]])` -> Number - index in {object} where {expr} is true
- [ ] `insert({object}, {item} [, {idx}])` -> List - insert {item} in {object} [before {idx}]
- [ ] `items({expr})` -> List - key/index-value pairs in {expr}
- [ ] `keys({dict})` -> List - keys in {dict}
- [x] `len({expr})` -> Number - the length of {expr}
- [ ] `list2blob({list})` -> Blob - turn {list} of numbers into a Blob
- [ ] `list2str({list} [, {utf8}])` -> String - turn {list} of numbers into a String
- [ ] `list2tuple({list})` -> Tuple - turn {list} of items into a tuple
- [ ] `map({expr1}, {expr2})` -> List/Dict/Blob/String - change each item in {expr1} to {expr2}
- [ ] `mapnew({expr1}, {expr2})` -> List/Dict/Blob/String - like |map()| but creates a new List or Dictionary
- [x] `max({expr})` -> Number - maximum value of items in {expr}
- [x] `min({expr})` -> Number - minimum value of items in {expr}
- [x] `range({expr} [, {max} [, {stride}]])` -> List - items from {expr} to {max}
- [ ] `readblob({fname} [, {offset} [, {size}]])` -> Blob - read a |Blob| from {fname}
- [ ] `reduce({object}, {func} [, {initial}])` -> any - reduce {object} using {func}
- [ ] `remove({blob}, {idx} [, {end}])` -> Number/Blob - remove bytes {idx}-{end} from {blob}
- [ ] `remove({dict}, {key})` -> any - remove entry {key} from {dict}
- [ ] `remove({list}, {idx} [, {end}])` - any/List remove items {idx}-{end} from {list}
- [ ] `repeat({expr}, {count})` -> List/Tuple/Blob/String - repeat {expr} {count} times
- [x] `reverse({obj})` -> List/Tuple/Blob/String - reverse {obj}
- [ ] `slice({expr}, {start} [, {end}])` -> String/List/Blob - slice of a String, List or Blob
- [x] `sort({list} [, {how} [, {dict}]])` -> List - sort {list}, compare with {how}
- [ ] `str2blob({list} [, {options}])` -> Blob - convert list of strings into a Blob
- [ ] `tuple2list({tuple})` -> List - turn {tuple} of items into a list
- [ ] `uniq({list} [, {func} [, {dict}]])` -> List - remove adjacent duplicates from a list
- [ ] `values({dict})` -> List - values in {dict}

## Window and Tabpage

- [ ] `gettabinfo([{expr}])` -> List - list of tab pages
- [ ] `gettabvar({nr}, {varname} [, {def}])` -> any - variable {varname} in tab {nr} or {def}
- [ ] `gettabwinvar({tabnr}, {winnr}, {name} [, {def}])` -> any - {name} in {winnr} in tab page {tabnr}
- [ ] `getwininfo([{winid}])` -> List - list of info about each window
- [ ] `getwinpos([{timeout}])` -> List - X and Y coord in pixels of Vim window
- [ ] `getwinposx()` -> Number - X coord in pixels of the Vim window
- [ ] `getwinposy()` -> Number - Y coord in pixels of the Vim window
- [ ] `getwinvar({nr}, {varname} [, {def}])` -> any - variable {varname} in window {nr}
- [ ] `settabvar({nr}, {varname}, {val})` -> none - set {varname} in tab page {nr} to {val}
- [ ] `settabwinvar({tabnr}, {winnr}, {varname}, {val})` -> none - set {varname} in window {winnr} in tab page {tabnr} to {val}
- [ ] `setwinvar({nr}, {varname}, {val})` -> none - set {varname} in window {nr} to {val}
- [ ] `tabpagebuflist([{arg}])` -> List - list of buffer numbers in tab page
- [ ] `tabpagenr([{arg}])` -> Number - number of current or last tab page
- [ ] `tabpagewinnr({tabarg} [, {arg}])` -> Number - number of current window in tab page
- [ ] `tabpanel_getinfo()` -> Dict - get current state of the tabpanel
- [ ] `tabpanel_scroll({n} [, {opts}])` -> Bool - scroll the tabpanel
- [ ] `win_execute({id}, {command} [, {silent}])` -> String - execute {command} in window {id}
- [ ] `win_findbuf({bufnr})` -> List - find windows containing {bufnr}
- [ ] `win_getid([{win} [, {tab}]])` -> Number - get window ID for {win} in {tab}
- [ ] `win_gettype([{nr}])` -> String - type of window {nr}
- [ ] `win_gotoid({expr})` -> Number - go to window with ID {expr}
- [ ] `win_id2tabwin({expr})` -> List - get tab and window nr from window ID
- [ ] `win_id2win({expr})` -> Number - get window nr from window ID
- [ ] `win_move_separator({nr})` -> Number - move window vertical separator
- [ ] `win_move_statusline({nr})` -> Number - move window status line
- [ ] `win_screenpos({nr})` -> List - get screen position of window {nr}
- [ ] `win_splitmove({nr}, {target} [, {options}])` -> Number - move window {nr} to split of {target}
- [ ] `winbufnr({nr})` -> Number - buffer number of window {nr}
- [ ] `wincol()` -> Number - window column of the cursor
- [ ] `windowsversion()` -> String - MS-Windows OS version
- [ ] `winheight({nr})` -> Number - height of window {nr}
- [ ] `winlayout([{tabnr}])` -> List - layout of windows in tab {tabnr}
- [ ] `winline()` -> Number - window line of the cursor
- [ ] `winnr([{expr}])` -> Number - number of current window
- [ ] `winrestcmd()` -> String - returns command to restore window sizes
- [ ] `winrestview({dict})` -> none - restore view of current window
- [ ] `winsaveview()` -> Dict - save view of current window
- [ ] `winwidth({nr})` -> Number - width of window {nr}

## System / OS / Filesystem

- [ ] `chdir({dir})` -> String - change current working directory
- [ ] `delete({fname} [, {flags}])` -> Number - delete the file or directory {fname}
- [ ] `environ()` -> Dict - return environment variables
- [ ] `executable({expr})` -> Number - 1 if executable {expr} exists
- [ ] `exepath({expr})` -> String - full path of the command {expr}
- [ ] `expand({expr} [, {nosuf} [, {list}]])` -> any - expand special keywords in {expr}
- [ ] `expandcmd({string} [, {options}])` -> String - expand {string} like with `:edit`
- [ ] `filecopy({from}, {to})` -> Number - |TRUE| if copying file {from} to {to} worked
- [ ] `filereadable({file})` -> Number - |TRUE| if {file} is a readable file
- [ ] `filewritable({file})` -> Number - |TRUE| if {file} is a writable file
- [ ] `finddir({name} [, {path} [, {count}]])` - 
- [ ] `findfile({name} [, {path} [, {count}]])` -> String/List - find dir/file {name} in {path}
- [ ] `getcwd([{winnr} [, {tabnr}]])` -> String - get the current working directory
- [ ] `getenv({name})` -> String - return environment variable
- [ ] `getfperm({fname})` -> String - file permissions of file {fname}
- [ ] `getfsize({fname})` -> Number - size in bytes of file {fname}
- [ ] `getftime({fname})` -> Number - last modification time of file
- [ ] `getftype({fname})` -> String - description of type of file {fname}
- [ ] `getpid()` -> Number - process ID of Vim
- [ ] `glob({expr} [, {nosuf} [, {list} [, {alllinks}]]])` -> any - expand file wildcards in {expr}
- [ ] `glob2regpat({expr})` -> String - convert a glob pat into a search pat
- [ ] `globpath({path}, {expr} [, {nosuf} [, {list} [, {alllinks}]]])` -> String - do glob({expr}) for all dirs in {path}
- [ ] `hostname()` -> String - name of the machine Vim is running on
- [ ] `isabsolutepath({path})` -> Number - |TRUE| if {path} is an absolute path
- [ ] `isdirectory({directory})` -> Number - |TRUE| if {directory} is a directory
- [ ] `libcall({lib}, {func}, {arg})` -> String - call {func} in library {lib} with {arg}
- [ ] `libcallnr({lib}, {func}, {arg})` -> Number - idem, but return a Number
- [ ] `localtime()` -> Number - current time
- [ ] `mkdir({name} [, {flags} [, {prot}]])` -> Number - create directory {name}
- [ ] `pathshorten({expr} [, {len}])` -> String - shorten directory names in a path
- [ ] `readdir({dir} [, {expr} [, {dict}]])` -> List - file names in {dir} selected by {expr}
- [ ] `readdirex({dir} [, {expr} [, {dict}]])` -> List - file info in {dir} selected by {expr}
- [ ] `readfile({fname} [, {type} [, {max}]])` -> List - get list of lines from file {fname}
- [ ] `rename({from}, {to})` -> Number - rename (move) file from {from} to {to}
- [ ] `resolve({filename})` -> String - get filename a shortcut points to
- [ ] `setenv({name}, {val})` -> none - set environment variable
- [ ] `setfperm({fname}, {mode})` -> Number - set {fname} file permissions to {mode}
- [ ] `shellescape({string} [, {special}])` -> String - escape {string} for use as shell command argument
- [ ] `simplify({filename})` -> String - simplify filename as much as possible
- [ ] `strftime({format} [, {time}])` -> String - format time with a specified format
- [ ] `strptime({format}, {timestring})` -> Number - convert {timestring} to unix timestamp
- [ ] `system({expr} [, {input}])` -> String - output of shell command/filter {expr}
- [ ] `systemlist({expr} [, {input}])` -> List - output of shell command/filter {expr}
- [ ] `tempname()` -> String - name for a temporary file
- [ ] `undofile({name})` -> String - undo file name for {name}
- [ ] `writefile({object}, {fname} [, {flags}])` -> Number - write |Blob| or |List| of lines to file

## Cursor and Marks

- [ ] `charcol({expr} [, {winid}])` -> Number - column number of cursor or mark
- [ ] `col({expr} [, {winid}])` -> Number - column byte index of cursor or mark
- [ ] `cursor({list})` -> Number - move cursor to position in {list}
- [ ] `cursor({lnum}, {col} [, {off}])` -> Number - move cursor to {lnum}, {col}, {off}
- [ ] `getcharpos({expr})` -> List - position of cursor, mark, etc.
- [ ] `getpos({expr})` -> List - position of cursor, mark, etc.
- [ ] `getregion({pos1}, {pos2} [, {opts}])` -> List - get the text from {pos1} to {pos2}
- [ ] `getregionpos({pos1}, {pos2} [, {opts}])` -> List - get a list of positions for a region
- [ ] `screenpos({winid}, {lnum}, {col})` -> Dict - screen row and col of a text character
- [ ] `search({pattern} [, {flags} [, {stopline} [, {timeout} [, {skip}]]]])` -> Number - search for {pattern}
- [ ] `searchcount([{options}])` -> Dict - get or update search stats
- [ ] `searchdecl({name} [, {global} [, {thisblock}]])` -> Number - search for variable declaration
- [ ] `searchpair({start}, {middle}, {end} [, {flags} [, {skip} [...]]])` -> Number - search for other end of start/end pair
- [ ] `searchpairpos({start}, {middle}, {end} [, {flags} [, {skip} [...]]])` -> List - search for other end of start/end pair
- [ ] `searchpos({pattern} [, {flags} [, {stopline} [, {timeout} [, {skip}]]]])` -> List - search for {pattern}
- [ ] `setcharpos({expr}, {list})` -> Number - set the {expr} position to {list}
- [ ] `setcharsearch({dict})` -> none - set character search from {dict}
- [ ] `setcursorcharpos({list})` -> Number - move cursor to position in {list}
- [ ] `setpos({expr}, {list})` -> Number - set the {expr} position to {list}
- [ ] `virtcol({expr} [, {list} [, {winid}])` -> Number/List - screen column of cursor or mark
- [ ] `virtcol2col({winid}, {lnum}, {col})` -> Number - byte index of a character on screen
- [ ] `visualmode([{expr}])` -> String - last visual mode used

## Editor State, Options, and Input

- [ ] `changenr()` -> Number - current change number
- [ ] `cmdcomplete_info()` -> Dict - get current cmdline completion information
- [ ] `complete({startcol}, {matches})` -> none - set Insert mode completion
- [ ] `complete_add({expr})` -> Number - add completion match
- [ ] `complete_check()` -> Number - check for key typed during completion
- [ ] `complete_info([{what}])` -> Dict - get current completion information
- [x] `exists({expr})` -> Number - |TRUE| if {expr} exists
- [ ] `exists_compiled({expr})` -> Number - |TRUE| if {expr} exists at compile time
- [ ] `getcmdcomplpat()` -> String - return the completion pattern of the current command-line completion
- [ ] `getcmdcompltype()` -> String - return the type of the current command-line completion
- [ ] `getcmdline()` -> String - return the current command-line input
- [ ] `getcmdpos()` -> Number - return cursor position in command-line
- [ ] `getcmdprompt()` -> String - return the current command-line prompt
- [ ] `getcmdscreenpos()` -> Number - return cursor screen position in command-line
- [ ] `getcmdtype()` -> String - return current command-line type
- [ ] `getcmdwintype()` -> String - return current command-line window type
- [ ] `getcompletion({pat}, {type} [, {filtered}])` -> List - list of cmdline completion matches
- [ ] `getcompletiontype({pat})` -> String - return the type of the command-line completion using {pat}
- [ ] `getcurpos([{winnr}])` -> List - position of the cursor
- [ ] `getcursorcharpos([{winnr}])` -> List - character position of the cursor
- [ ] `getimstatus()` -> Number - |TRUE| if the IME status is active
- [ ] `getjumplist([{winnr} [, {tabnr}]])` -> List - list of jump list items
- [ ] `getloclist({nr})` -> List - list of location list items
- [ ] `getloclist({nr}, {what})` -> Dict - get specific location list properties
- [ ] `getmarklist([{buf}])` -> List - list of global/local marks
- [ ] `getmousepos()` -> Dict - last known mouse position
- [ ] `getmouseshape()` -> String - current mouse shape name
- [ ] `getqflist()` -> List - list of quickfix items
- [ ] `getqflist({what})` -> Dict - get specific quickfix list properties
- [ ] `getreg([{regname} [, 1 [, {list}]]])` -> String/List - contents of a register
- [ ] `getreginfo([{regname}])` -> Dict - information about a register
- [ ] `getregtype([{regname}])` -> String - type of a register
- [ ] `has({feature} [, {check}])` -> Number - |TRUE| if feature {feature} supported
- [ ] `haslocaldir([{winnr} [, {tabnr}]])` -> Number - |TRUE| if the window executed `:lcd` or `:tcd`
- [ ] `hasmapto({what} [, {mode} [, {abbr}]])` -> Number - |TRUE| if mapping to {what} exists
- [ ] `histadd({history}, {item})` -> Number - add an item to a history
- [ ] `histdel({history} [, {item}])` -> Number - remove an item from a history
- [ ] `histget({history} [, {index}])` -> String - get the item {index} from a history
- [ ] `histnr({history})` -> Number - highest index of a history
- [ ] `mode([{expr}])` -> String - current editing mode
- [ ] `prop_add({lnum}, {col}, {props})` -> Number - add one text property
- [ ] `prop_add_list({props}, [[{lnum}, {col}, {end-lnum}, {end-col}], ...])` -> none - add multiple text properties
- [ ] `prop_clear({lnum} [, {lnum-end} [, {props}]])` -> none - remove all text properties
- [ ] `prop_find({props} [, {direction}])` -> Dict - search for a text property
- [ ] `prop_list({lnum} [, {props}])` -> List - text properties in {lnum}
- [ ] `prop_remove({props} [, {lnum} [, {lnum-end}]])` -> Number - remove a text property
- [ ] `prop_type_add({name}, {props})` -> none - define a new property type
- [ ] `prop_type_change({name}, {props})` -> none - change an existing property type
- [ ] `prop_type_delete({name} [, {props}])` -> none - delete a property type
- [ ] `prop_type_get({name} [, {props}])` -> Dict - get property type values
- [ ] `prop_type_list([{props}])` -> List - get list of property types
- [ ] `pum_getpos()` -> Dict - position and size of pum if visible
- [ ] `pumvisible()` -> Number - whether popup menu is visible
- [ ] `reg_executing()` -> String - get the executing register name
- [ ] `reg_recording()` -> String - get the recording register name
- [ ] `screenattr({row}, {col})` -> Number - attribute at screen position
- [ ] `screenchar({row}, {col})` -> Number - character at screen position
- [ ] `screenchars({row}, {col})` -> List - list of characters at screen position
- [ ] `screencol()` -> Number - current cursor column
- [ ] `screenrow()` -> Number - current cursor row
- [ ] `screenstring({row}, {col})` -> String - characters at screen position
- [ ] `setcmdline({str} [, {pos}])` -> Number - set command-line
- [ ] `setcmdpos({pos})` -> Number - set cursor position in command-line
- [ ] `setloclist({nr}, {list} [, {action}])` -> Number - modify location list using {list}
- [ ] `setloclist({nr}, {list}, {action}, {what})` -> Number - modify specific location list props
- [ ] `setqflist({list} [, {action}])` -> Number - modify quickfix list using {list}
- [ ] `setqflist({list}, {action}, {what})` -> Number - modify specific quickfix list props
- [ ] `setreg({n}, {v} [, {opt}])` -> Number - set register to value and type
- [ ] `shiftwidth([{col}])` -> Number - effective value of 'shiftwidth'
- [ ] `sign_define({list})` -> List - define or update a list of signs
- [ ] `sign_define({name} [, {dict}])` -> Number - define or update a sign
- [ ] `sign_getdefined([{name}])` -> List - get a list of defined signs
- [ ] `sign_getplaced([{buf} [, {dict}]])` -> List - get a list of placed signs
- [ ] `sign_jump({id}, {group}, {buf})` -> Number - jump to a sign
- [ ] `sign_place({id}, {group}, {name}, {buf} [, {dict}])` -> Number - place a sign
- [ ] `sign_placelist({list})` -> List - place a list of signs
- [ ] `sign_undefine([{name}])` -> Number - undefine a sign
- [ ] `sign_undefine({list})` -> List - undefine a list of signs
- [ ] `sign_unplace({group} [, {dict}])` -> Number - unplace a sign
- [ ] `sign_unplacelist({list})` -> List - unplace a list of signs
- [ ] `state([{what}])` -> String - current state of Vim
- [ ] `swapfilelist()` -> List - swap files found in 'directory'
- [ ] `swapinfo({fname})` -> Dict - information about swap file {fname}
- [ ] `swapname({buf})` -> String - swap file of buffer {buf}
- [ ] `tagfiles()` -> List - tags files used
- [ ] `taglist({expr} [, {filename}])` -> List - list of tags matching {expr}
- [ ] `undotree([{buf}])` -> Dict - undo file tree for buffer {buf}
- [ ] `wildmenumode()` -> Number - whether 'wildmenu' mode is active
- [ ] `wildtrigger()` -> none - start wildcard expansion
- [ ] `wordcount()` -> Dict - get byte/char/word statistics

## Syntax, Highlight, and Spell

- [ ] `clearmatches([{win}])` -> none - clear all matches
- [ ] `digraph_get({chars})` -> String - get the |digraph| of {chars}
- [ ] `digraph_getlist([{listall}])` -> List - get all |digraph|s
- [ ] `digraph_set({chars}, {digraph})` -> Bool - register |digraph|
- [ ] `digraph_setlist({digraphlist})` -> Bool - register multiple |digraph|s
- [ ] `getmatches([{win}])` -> List - list of current matches
- [ ] `hlID({name})` -> Number - syntax ID of highlight group {name}
- [ ] `hlexists({name})` -> Number - |TRUE| if highlight group {name} exists
- [ ] `hlget([{name} [, {resolve}]])` -> List - get highlight group attributes
- [ ] `hlset({list})` -> Number - set highlight group attributes
- [ ] `match({expr}, {pat} [, {start} [, {count}]])` -> Number - position where {pat} matches in {expr}
- [ ] `matchadd({group}, {pattern} [, {priority} [, {id} [, {dict}]]])` -> Number - highlight {pattern} with {group}
- [ ] `matchaddpos({group}, {pos} [, {priority} [, {id} [, {dict}]]])` -> Number - highlight positions with {group}
- [ ] `matcharg({nr})` -> List - arguments of `:match`
- [ ] `matchbufline({buf}, {pat}, {lnum}, {end}, [, {dict})` -> List - all the {pat} matches in buffer {buf}
- [ ] `matchdelete({id} [, {win}])` -> Number - delete match identified by {id}
- [ ] `matchend({expr}, {pat} [, {start} [, {count}]])` -> Number - position where {pat} ends in {expr}
- [ ] `matchfuzzy({list}, {str} [, {dict}])` -> List - fuzzy match {str} in {list}
- [ ] `matchfuzzypos({list}, {str} [, {dict}])` -> List - fuzzy match {str} in {list}
- [ ] `matchlist({expr}, {pat} [, {start} [, {count}]])` -> List - match and submatches of {pat} in {expr}
- [ ] `matchstr({expr}, {pat} [, {start} [, {count}]])` -> String - {count}'th match of {pat} in {expr}
- [ ] `matchstrlist({list}, {pat} [, {dict})` -> List - all the {pat} matches in {list}
- [ ] `matchstrpos({expr}, {pat} [, {start} [, {count}]])` -> List - {count}'th match of {pat} in {expr}
- [ ] `setmatches({list} [, {win}])` -> Number - restore a list of matches
- [ ] `spellbadword()` -> List - badly spelled word at cursor
- [ ] `spellsuggest({word} [, {max} [, {capital}]])` -> List - spelling suggestions
- [ ] `synID({lnum}, {col}, {trans})` -> Number - syntax ID at {lnum} and {col}
- [ ] `synIDattr({synID}, {what} [, {mode}])` -> String - attribute {what} of syntax ID {synID}
- [ ] `synIDtrans({synID})` -> Number - translated syntax ID of {synID}
- [ ] `synconcealed({lnum}, {col})` -> List - info about concealing
- [ ] `synstack({lnum}, {col})` -> List - stack of syntax IDs at {lnum} and {col}

## Autocmd, Events, and Timers

- [ ] `autocmd_add({acmds})` -> Bool - add a list of autocmds and groups
- [ ] `autocmd_delete({acmds})` -> Bool - delete a list of autocmds and groups
- [ ] `autocmd_get([{opts}])` -> List - return a list of autocmds
- [ ] `did_filetype()` -> Number - |TRUE| if FileType autocmd event used
- [ ] `eventhandler()` -> Number - |TRUE| if inside an event handler
- [ ] `listener_add({callback} [, {buf} [, {unbuffered}]])` -> Number - add a callback to listen to changes
- [ ] `listener_flush([{buf}])` -> none - invoke listener callbacks
- [ ] `listener_remove({id})` -> Number - remove a listener callback
- [ ] `timer_info([{id}])` -> List - information about timers
- [ ] `timer_pause({id}, {pause})` -> none - pause or unpause a timer
- [ ] `timer_start({time}, {callback} [, {options}])` -> Number - create a timer
- [ ] `timer_stop({timer})` -> none - stop a timer
- [ ] `timer_stopall()` -> none - stop all timers

## Job, Channel, Terminal, and IPC

- [ ] `ch_canread({handle})` -> Number - check if there is something to read
- [ ] `ch_close({handle})` -> none - close {handle}
- [ ] `ch_close_in({handle})` -> none - close in part of {handle}
- [ ] `ch_evalexpr({handle}, {expr} [, {options}])` -> any - evaluate {expr} on JSON {handle}
- [ ] `ch_evalraw({handle}, {string} [, {options}])` -> any - evaluate {string} on raw {handle}
- [ ] `ch_getbufnr({handle}, {what})` -> Number - get buffer number for {handle}/{what}
- [ ] `ch_getjob({channel})` -> Job - get the Job of {channel}
- [ ] `ch_info({handle})` -> Dict - info about channel {handle}
- [ ] `ch_listen({address} [, {options}])` -> Channel - listen on {address} - port on loopback or UNIX domain socket
- [ ] `ch_log({msg} [, {handle}])` -> none - write {msg} in the channel log file
- [ ] `ch_logfile({fname} [, {mode}])` -> none - start logging channel activity
- [ ] `ch_open({address} [, {options}])` -> Channel - open a channel to {address}
- [ ] `ch_read({handle} [, {options}])` -> String - read from {handle}
- [ ] `ch_readblob({handle} [, {options}])` -> Blob - read Blob from {handle}
- [ ] `ch_readraw({handle} [, {options}])` -> String - read raw from {handle}
- [ ] `ch_sendexpr({handle}, {expr} [, {options}])` -> any - send {expr} over JSON {handle}
- [ ] `ch_sendraw({handle}, {expr} [, {options}])` -> none - send {expr} over raw {handle}
- [ ] `ch_setoptions({handle}, {options})` -> none - set options for {handle}
- [ ] `ch_status({handle} [, {options}])` -> String - status of channel {handle}
- [ ] `job_getchannel({job})` -> Channel - get the channel handle for {job}
- [ ] `job_info([{job}])` -> Dict - get information about {job}
- [ ] `job_setoptions({job}, {options})` -> none - set options for {job}
- [ ] `job_start({command} [, {options}])` -> Job - start a job
- [ ] `job_status({job})` -> String - get the status of {job}
- [ ] `job_stop({job} [, {how}])` -> Number - stop {job}
- [ ] `remote_expr({server}, {string} [, {idvar} [, {timeout}]])` -> String - send expression
- [ ] `remote_foreground({server})` -> none - bring Vim server to the foreground
- [ ] `remote_peek({serverid} [, {retvar}])` -> Number - check for reply string
- [ ] `remote_read({serverid} [, {timeout}])` -> String - read reply string
- [ ] `remote_send({server}, {string} [, {idvar}])` -> String - send key sequence
- [ ] `remote_startserver({name})` -> none - become server {name}
- [ ] `server2client({clientid}, {string})` -> Number - send reply string
- [ ] `serverlist()` -> String - get a list of available servers
- [ ] `term_dumpdiff({filename}, {filename} [, {options}])` -> Number - display difference between two dumps
- [ ] `term_dumpload({filename} [, {options}])` -> Number - displaying a screen dump
- [ ] `term_dumpwrite({buf}, {filename} [, {options}])` -> none - dump terminal window contents
- [ ] `term_getaltscreen({buf})` -> Number - get the alternate screen flag
- [ ] `term_getansicolors({buf})` -> List - get ANSI palette in GUI color mode
- [ ] `term_getattr({attr}, {what})` -> Number - get the value of attribute {what}
- [ ] `term_getcursor({buf})` -> List - get the cursor position of a terminal
- [ ] `term_getjob({buf})` -> Job - get the job associated with a terminal
- [ ] `term_getline({buf}, {row})` -> String - get a line of text from a terminal
- [ ] `term_getscrolled({buf})` -> Number - get the scroll count of a terminal
- [ ] `term_getsize({buf})` -> List - get the size of a terminal
- [ ] `term_getstatus({buf})` -> String - get the status of a terminal
- [ ] `term_gettitle({buf})` -> String - get the title of a terminal
- [ ] `term_gettty({buf}, [{input}])` -> String - get the tty name of a terminal
- [ ] `term_list()` -> List - get the list of terminal buffers
- [ ] `term_scrape({buf}, {row})` -> List - get row of a terminal screen
- [ ] `term_sendkeys({buf}, {keys})` -> none - send keystrokes to a terminal
- [ ] `term_setansicolors({buf}, {colors})` -> none - set ANSI palette in GUI color mode
- [ ] `term_setapi({buf}, {expr})` -> none - set |terminal-api| function name prefix
- [ ] `term_setkill({buf}, {how})` -> none - set signal to stop job in terminal
- [ ] `term_setrestore({buf}, {command})` -> none - set command to restore terminal
- [ ] `term_setsize({buf}, {rows}, {cols})` -> none - set the size of a terminal
- [ ] `term_start({cmd} [, {options}])` -> Number - open a terminal window and run a job
- [ ] `term_wait({buf} [, {time}])` -> none - wait for screen to be updated
- [ ] `terminalprops()` -> Dict - properties of the terminal

## GUI, Popup, Menu, and Balloons

- [ ] `balloon_gettext()` -> String - current text in the balloon
- [ ] `balloon_show({expr})` -> none - show {expr} inside the balloon
- [ ] `balloon_split({msg})` -> List - split {msg} as used for a balloon
- [ ] `browse({save}, {title}, {initdir}, {default})` -> String - put up a file requester
- [ ] `browsedir({title}, {initdir})` -> String - put up a directory requester
- [ ] `confirm({msg} [, {choices} [, {default} [, {type}]]])` -> Number - number of choice picked by user
- [ ] `foreground()` -> none - bring the Vim window to the foreground
- [ ] `getbgcolor()` -> List - get background colour as [r, g, b]
- [ ] `getcellpixels()` -> List - get character cell pixel size
- [ ] `getcellwidths()` -> List - get character cell width overrides
- [ ] `input({prompt} [, {text} [, {completion}]])` -> String - get input from the user
- [ ] `inputdialog({prompt} [, {text} [, {cancelreturn}]])` -> String - like input() but in a GUI dialog
- [ ] `inputlist({textlist})` -> Number - let the user pick from a choice list
- [ ] `inputrestore()` -> Number - restore typeahead
- [ ] `inputsave()` -> Number - save and clear typeahead
- [ ] `inputsecret({prompt} [, {text}])` -> String - like input() but hiding the text
- [ ] `menu_info({name} [, {mode}])` -> Dict - get menu item information
- [ ] `popup_atcursor({what}, {options})` -> Number - create popup window near the cursor
- [ ] `popup_beval({what}, {options})` -> Number - create popup window for 'ballooneval'
- [ ] `popup_clear()` -> none - close all popup windows
- [ ] `popup_close({id} [, {result}])` -> none - close popup window {id}
- [ ] `popup_create({what}, {options})` -> Number - create a popup window
- [ ] `popup_dialog({what}, {options})` -> Number - create a popup window used as a dialog
- [ ] `popup_filter_menu({id}, {key})` -> Bool - filter for a menu popup window
- [ ] `popup_filter_yesno({id}, {key})` -> Bool - filter for a dialog popup window
- [ ] `popup_findecho()` -> Number - get window ID of popup for `:echowin`
- [ ] `popup_findinfo()` -> Number - get window ID of info popup window
- [ ] `popup_findpreview()` -> Number - get window ID of preview popup window
- [ ] `popup_getoptions({id})` -> Dict - get options of popup window {id}
- [ ] `popup_getpos({id})` -> Dict - get position of popup window {id}
- [ ] `popup_hide({id})` -> none - hide popup menu {id}
- [ ] `popup_list()` -> List - get a list of window IDs of all popups
- [ ] `popup_locate({row}, {col})` -> Number - get window ID of popup at position
- [ ] `popup_menu({what}, {options})` -> Number - create a popup window used as a menu
- [ ] `popup_move({id}, {options})` -> none - set position of popup window {id}
- [ ] `popup_notification({what}, {options})` -> Number - create a notification popup window
- [ ] `popup_setbuf({id}, {buf})` -> Bool - set the buffer for the popup window {id}
- [ ] `popup_setoptions({id}, {options})` -> none - set options for popup window {id}
- [ ] `popup_settext({id}, {text})` -> none - set the text of popup window {id}
- [ ] `popup_show({id})` -> Number - unhide popup window {id}
- [ ] `setcellwidths({list})` -> none - set character cell width overrides

## Language Evaluation and Types

- [ ] `call({func}, {arglist} [, {dict}])` -> any - call {func} with arguments {arglist}
- [ ] `eval({string})` -> any - evaluate {string} into its value
- [ ] `funcref({name} [, {arglist}] [, {dict}])` -> Funcref - reference to function {name}
- [ ] `function({name} [, {arglist}] [, {dict}])` -> Funcref - named reference to function {name}
- [ ] `garbagecollect([{atexit}])` -> none - free memory, breaking cyclic references
- [ ] `getscriptinfo([{opts}])` -> List - list of sourced scripts
- [ ] `getstacktrace()` -> List - get current stack trace of Vim scripts
- [ ] `instanceof({object}, {class})` -> Bool - |TRUE| if {object} is an instance of {class}
- [ ] `interrupt()` -> none - interrupt script execution
- [ ] `islocked({expr})` -> Number - |TRUE| if {expr} is locked
- [ ] `luaeval({expr} [, {expr}])` -> any - evaluate |Lua| expression
- [ ] `mzeval({expr})` -> any - evaluate |MzScheme| expression
- [ ] `perleval({expr})` -> any - evaluate |Perl| expression
- [ ] `py3eval({expr} [, {locals}])` -> any - evaluate |python3| expression
- [ ] `pyxeval({expr} [, {locals}])` -> any - evaluate |python_x| expression
- [ ] `rubyeval({expr})` -> any - evaluate |Ruby| expression
- [x] `type({expr})` -> Number - type of value {expr}
- [ ] `typename({expr})` -> String - representation of the type of {expr}

## Testing, Debugging, and Assertions

- [ ] `assert_beeps({cmd})` -> Number - assert {cmd} causes a beep
- [ ] `assert_equal({exp}, {act} [, {msg}])` -> Number - assert {exp} is equal to {act}
- [ ] `assert_equalfile({fname-one}, {fname-two} [, {msg}])` -> Number - assert file contents are equal
- [ ] `assert_exception({error} [, {msg}])` -> Number - assert {error} is in |v:exception|
- [ ] `assert_fails({cmd} [, {error} [, {msg} [, {lnum} [, {context}]]]])` -> Number - assert {cmd} fails
- [ ] `assert_false({actual} [, {msg}])` -> Number - assert {actual} is false
- [ ] `assert_inrange({lower}, {upper}, {actual} [, {msg}])` -> Number - assert {actual} is inside the range
- [ ] `assert_match({pat}, {text} [, {msg}])` -> Number - assert {pat} matches {text}
- [ ] `assert_nobeep({cmd})` -> Number - assert {cmd} does not cause a beep
- [ ] `assert_notequal({exp}, {act} [, {msg}])` -> Number - assert {exp} is not equal {act}
- [ ] `assert_notmatch({pat}, {text} [, {msg}])` -> Number - assert {pat} not matches {text}
- [ ] `assert_report({msg})` -> Number - report a test failure
- [ ] `assert_true({actual} [, {msg}])` -> Number - assert {actual} is true
- [ ] `debugbreak({pid})` -> Number - interrupt process being debugged
- [ ] `test_alloc_fail({id}, {countdown}, {repeat})` -> none - make memory allocation fail
- [ ] `test_autochdir()` -> none - enable 'autochdir' during startup
- [ ] `test_feedinput({string})` -> none - add key sequence to input buffer
- [ ] `test_garbagecollect_now()` -> none - free memory right now for testing
- [ ] `test_garbagecollect_soon()` -> none - free memory soon for testing
- [ ] `test_getvalue({string})` -> Number - get value of an internal variable
- [ ] `test_gui_event({event}, {args})` -> bool - generate a GUI event for testing
- [ ] `test_ignore_error({expr})` -> none - ignore a specific error
- [ ] `test_mswin_event({event}, {args})` -> Bool - generate MS-Windows event for testing
- [ ] `test_null_blob()` -> Blob - null value for testing
- [ ] `test_null_channel()` -> Channel - null value for testing
- [ ] `test_null_dict()` -> Dict - null value for testing
- [ ] `test_null_function()` -> Funcref - null value for testing
- [ ] `test_null_job()` -> Job - null value for testing
- [ ] `test_null_list()` -> List - null value for testing
- [ ] `test_null_partial()` -> Funcref - null value for testing
- [ ] `test_null_string()` -> String - null value for testing
- [ ] `test_null_tuple()` -> Tuple - null value for testing
- [ ] `test_option_not_set({name})` -> none - reset flag indicating option was set
- [ ] `test_override({expr}, {val})` -> none - test with Vim internal overrides
- [ ] `test_refcount({expr})` -> Number - get the reference count of {expr}
- [ ] `test_setmouse({row}, {col})` -> none - set the mouse position for testing
- [ ] `test_settime({expr})` -> none - set current time for testing
- [ ] `test_srand_seed([{seed}])` -> none - set seed for testing srand()
- [ ] `test_unknown()` -> any - unknown value for testing
- [ ] `test_void()` -> none - void value for testing

## Other / Uncategorized

- [ ] `` - 
- [ ] `argc([{winid}])` -> Number - number of files in the argument list
- [ ] `argidx()` -> Number - current index in the argument list
- [ ] `arglistid([{winnr} [, {tabnr}]])` -> Number - argument list id
- [ ] `argv([-1, {winid}])` -> List - the argument list
- [ ] `argv({nr} [, {winid}])` -> String - {nr} entry of the argument list
- [ ] `bindtextdomain({package}, {path})` -> Bool - bind text domain to specified path
- [ ] `byteidx({expr}, {nr} [, {utf16}])` -> Number - byte index of {nr}'th char in {expr}
- [ ] `byteidxcomp({expr}, {nr} [, {utf16}])` -> Number - byte index of {nr}'th char in {expr}
- [ ] `cindent({lnum})` -> Number - C indent for line {lnum}
- [ ] `cscope_connection([{num}, {dbpath} [, {prepend}]])` -> Number - checks existence of cscope connection
- [ ] `diff({fromlist}, {tolist} [, {options}])` -> List - diff two Lists of strings
- [ ] `diff_filler({lnum})` -> Number - diff filler lines about {lnum}
- [ ] `diff_hlID({lnum}, {col})` -> Number - diff highlighting at {lnum}/{col}
- [ ] `echoraw({expr})` -> none - output {expr} as-is
- [ ] `err_teapot([{expr}])` -> none - give E418, or E503 if {expr} is |TRUE|
- [ ] `execute({command})` -> String - execute {command} and get the output
- [ ] `feedkeys({string} [, {mode}])` -> none - add key sequence to typeahead buffer
- [ ] `fnamemodify({fname}, {mods})` -> String - modify file name
- [ ] `foldclosed({lnum})` -> Number - first line of fold at {lnum} if closed
- [ ] `foldclosedend({lnum})` -> Number - last line of fold at {lnum} if closed
- [ ] `foldlevel({lnum})` -> Number - fold level at {lnum}
- [ ] `foldtext()` -> String - line displayed for closed fold
- [ ] `foldtextresult({lnum})` -> String - text for closed fold at {lnum}
- [ ] `fullcommand({name} [, {vim9}])` -> String - get full command from {name}
- [ ] `getchangelist([{buf}])` -> List - list of change list items
- [ ] `getchar([{expr} [, {opts}]])` -> Number/String - get one character from the user
- [ ] `getcharmod()` -> Number - modifiers for the last typed character
- [ ] `getcharsearch()` -> Dict - last character search
- [ ] `getcharstr([{expr} [, {opts}]])` -> String - get one character from the user
- [ ] `getfontname([{name}])` -> String - name of font being used
- [ ] `gettagstack([{nr}])` -> Dict - get the tag stack of window {nr}
- [ ] `gettext({text} [, {package}])` -> String - lookup translation of {text}
- [ ] `id({item})` -> String - get unique identity string of item
- [ ] `indent({lnum})` -> Number - indent of line {lnum}
- [ ] `js_decode({string})` -> any - decode JS style JSON
- [ ] `js_encode({expr})` -> String - encode JS style JSON
- [ ] `json_decode({string})` -> any - decode JSON
- [ ] `json_encode({expr})` -> String - encode JSON
- [ ] `lispindent({lnum})` -> Number - Lisp indent for line {lnum}
- [ ] `maparg({name} [, {mode} [, {abbr} [, {dict}]]])` -> String/Dict - rhs of mapping {name} in mode {mode}
- [ ] `mapcheck({name} [, {mode} [, {abbr}]])` -> String - check for mappings matching {name}
- [ ] `maplist([{abbr}])` -> List - list of all mappings, a dict for each
- [ ] `mapset({mode}, {abbr}, {dict})` -> none - restore mapping from |maparg()| result
- [ ] `nextnonblank({lnum})` -> Number - line nr of non-blank line >= {lnum}
- [ ] `ngettext({single}, {plural}, {number}[, {domain}])` -> String - translate text based on {number}
- [ ] `preinserted()` -> Number - whether text is inserted after cursor
- [ ] `prevnonblank({lnum})` -> Number - line nr of non-blank line <= {lnum}
- [ ] `prompt_getprompt({buf})` -> String - get prompt text
- [ ] `prompt_setcallback({buf}, {expr})` -> none - set prompt callback function
- [ ] `prompt_setinterrupt({buf}, {text})` -> none - set prompt interrupt function
- [ ] `prompt_setprompt({buf}, {text})` -> none - set prompt text
- [ ] `pyeval({expr} [, {locals}])` -> any - evaluate |Python| expression
- [ ] `redraw_listener_add({opts})` -> Number - add callbacks to listen for redraws
- [ ] `redraw_listener_remove({id})` -> none - remove a redraw listener
- [ ] `reltime([{start} [, {end}]])` -> List - get time value
- [ ] `reltimefloat({time})` -> Float - turn the time value into a Float
- [ ] `reltimestr({time})` -> String - turn time value into a String
- [ ] `settagstack({nr}, {dict} [, {action}])` -> Number - modify tag stack using {dict}
- [ ] `sha256({expr})` -> String - SHA256 checksum of String or Blob
- [ ] `sound_clear()` -> none - stop playing all sounds
- [ ] `sound_playevent({name} [, {callback}])` -> Number - play an event sound
- [ ] `sound_playfile({path} [, {callback}])` -> Number - play sound file {path}
- [ ] `sound_stop({id})` -> none - stop playing sound {id}
- [x] `string({expr})` -> String - String representation of {expr} value

