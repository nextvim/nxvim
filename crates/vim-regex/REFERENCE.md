Title: Vim Regular Expressions 101

Source: https://vimregex.com/

---

[news](https://vimregex.com/#news)
[intro](https://vimregex.com/#intro)
[substitute](https://vimregex.com/#substitute)
[global](https://vimregex.com/#global)
[patterns](https://vimregex.com/#pattern)
[examples](https://vimregex.com/#examples)
[other flavors](https://vimregex.com/#compare)
[links](https://vimregex.com/#links)

# Contents
[I. News](https://vimregex.com/#news) [II. Introduction](https://vimregex.com/#intro) [2.1 What is VIM?](https://vimregex.com/#whatisvim) [2.2 About this Tutorial](https://vimregex.com/#about) [2.3 Credits](https://vimregex.com/#credits) [III. Substitute Command](https://vimregex.com/#substitute) [3.1 Search & Replace](https://vimregex.com/#substitute) [3.2 Line Ranges & Addressing](https://vimregex.com/#address) [IV. Pattern Description](https://vimregex.com/#pattern) [4.1 Anchors](https://vimregex.com/#anchors) [4.2 "Escaped" characters or metacharacters](https://vimregex.com/#metacharacters) [4.3 Quantifiers, Greedy and Non-Greedy](https://vimregex.com/#Non-Greedy) [4.4 Character ranges](https://vimregex.com/#ranges) [4.5 Grouping and Backreferences](https://vimregex.com/#backreferences) [4.6 Alternations](https://vimregex.com/#alternations) [4.7 Operator Precedence](https://vimregex.com/#precedence) [V. Global Command](https://vimregex.com/#global) [5.1 Global search and execution](https://vimregex.com/#global%20search) [5.2 Examples](https://vimregex.com/#global%20examples) [VI. Examples](https://vimregex.com/#examples) [6.1 Tips & Techniques](https://vimregex.com/#tips) [6.2 Creating Outline](https://vimregex.com/#contents) [6.3 Working with Tables](https://vimregex.com/#tables) [VII. Other Regexp Flavors](https://vimregex.com/#compare) [VIII. Links](https://vimregex.com/#links)

# [I. News](https://vimregex.com/)
[I. News](https://vimregex.com/)
- 
	  This page has been moved from an old geocities to rescue it from a premature death.
	  If you are the former maintainer of this content please [contact us](http://montrosegroupinc.com/contact.html) (we tried to contact you, honest)
	  to let us know if you are interested in resume maintainence of this content or just to say Cheers!

[contact us](http://montrosegroupinc.com/contact.html)

# [II. Introduction](https://vimregex.com/)
[II. Introduction](https://vimregex.com/)

## [2.1 What is VIM?](https://vimregex.com/)
[2.1 What is VIM?](https://vimregex.com/)
Vim is an improved (in many ways) version of vi, a ubiquitous text editor found on any UNIX system. VIM was created by [Bram Moolenaar](https://vimregex.com/images/photos/nyc/pics/linux-bram.jpg) with a help of other people. It's free but if you like it you can make a charitable contribution to orphans in Uganda.
Vim has its own web site, [www.vim.org](http://www.vim.org) and several [mailing lists](http://www.vim.org/mail.html), with a wealth of information on every aspect of VIM. Vim was successfully ported to nearly all existing OS. It is a default editor in many Linux distributions (e.g. RedHat).
VIM has all features of a modern programmer's editor - macro language, syntax highlighting, customizable user interface, easy integration with various IDEs plus a set of features which makes VIM so attractive to its users: crash recovery, automatic commands, session management.
VIM has a very broad and loyal user base. Over 10 million people have it installed (counting only Linux users). Estimation is that there are about half a million people using Vim as their main editor. And this number is growing.

## [2.2 About this Tutorial](https://vimregex.com/)
[2.2 About this Tutorial](https://vimregex.com/)
I started this tutorial for one simple reason - I like regular expressions. Nothing compares to the satisfaction from a well-crafted regexp which does exactly what you wanted it to do :-). I hope it's passable as a foreword.
Speaking more seriously, regular expressions (or regexps for short) are tools used to manipulate text and data. They don't exist as a standalone product but usually are a part of some program/utility. The best known example is UNIX grep, a program to search files for lines that match certain pattern. The search pattern is described in terms of regular expressions. You can think of regexps as a specialized pattern language. Regexps are quite useful and can greatly reduce time it takes to do some tedious text editing.
(Regexp terminology is largely borrowed from Jeffrey Friedl "Mastering Regular Expressions.")

## [2.3 Credits](https://vimregex.com/)
[2.3 Credits](https://vimregex.com/)
Many thanks (in no particular order): Benji Fisher, Zdenek Sekera, Preben "Peppe" Guldberg, Steve Kirkendall, Shaul Karl and all others who helped me with their comments.
mailto:volontir at yahoo dot comFeel free to send me (volontir at yahoo dot com) your comments. suggestions, examples...

[III. Substitute Command](https://vimregex.com/)

## [3.1 Search & Replace](https://vimregex.com/)
[3.1 Search & Replace](https://vimregex.com/)
So, what can you do with regular expressions? The most common task is to make replacements in a text following some certain rules. For this tutorial you need to know VIM search and replace command (S&R) :substitute. Here is an excerpt from VIM help:

```
:substitute
```

Part of the command word enclosed in the "[" & "]" can be omitted.

## [3.2 Range of Operation, Line Addressing and Marks](https://vimregex.com/)
[3.2 Range of Operation, Line Addressing and Marks](https://vimregex.com/)
Before I begin with a pattern description let's talk about line addresses in Vim. Some Vim commands can accept a line range in front of them. By specifying the line range you restrict the command execution to this particular part of text only. Line range consists of one or more line specifiers, separated with a comma or semicolon. You can also mark your current position in the text typing ml , where "l" can be any letter, and use it later defining the line address.

```
ml
```

If no line range is specified the command will operate on the current line only.
Here are a few examples:
10,20

```
10,20
```

- from 10 to 20 line.
Each may be followed (several times) by "+" or "-" and an optional number. This number is added or subtracted from the preceding line number. If the number is omitted, 1 is used.
/Section 1/+,/Section 2/-

```
/Section 1/+,/Section 2/-
```

- all lines between Section 1 and Section 2, non-inclusively, i.e. the lines containing Section 1 and Section 2 will not be affected.
The /pattern/ and ?pattern? may be followed by another address separated by a semicolon. A semicolon between two search patterns tells Vim to find the location of the first pattern, then start searching from that location for the second pattern.

```
/pattern/
```


```
?pattern?
```

/Section 1/;/Subsection/-,/Subsection/+

```
/Section 1/;/Subsection/-,/Subsection/+
```

- first find Section 1, then the first line with Subsection, step one line down (beginning of the range) and find the next line with Subsection, step one line up (end of the range).
The next example shows how you can reuse you search pattern:
:/Section/+ y

```
:/Section/+ y
```

- this will search for the Section line and yank (copy) one line after into the memory.
:// normal p

```
:// normal p
```

- and that will search for the next Section line and put (paste) the saved text on the next line.
Tip 1: frequently you need to do S&R in a text which contains UNIX file paths - text strings with slashes ("/") inside. Because S&R command uses slashes for pattern/replacement separation you have to escape every slash in your pattern, i.e. use "\/" for every "/" in your pattern:
s/\/dir1\/dir2\/dir3\/file/dir4\/dir5\/file2/g

```
s/\/dir1\/dir2\/dir3\/file/dir4\/dir5\/file2/g
```

To avoid this so-called "backslashitis" you can use different separators in S&R (I prefer ":")
s:/dir1/dir2/dir3/file:/dir4/dir5/file2:g

```
s:/dir1/dir2/dir3/file:/dir4/dir5/file2:g
```

Tip 2: You may find these mappings useful (put them in your .vimrc file)
noremap ;; :%s:::g<Left><Left><Left> noremap ;' :%s:::cg<Left><Left><Left><Left>

```
noremap ;; :%s:::g<Left><Left><Left> noremap ;' :%s:::cg<Left><Left><Left><Left>
```

These mappings save you some keystrokes and put you where you start typing your search pattern. After typing it you move to the replacement part , type it and hit return. The second version adds confirmation flag.

[IV. Pattern Description](https://vimregex.com/)

## [4.1 Anchors](https://vimregex.com/)
[4.1 Anchors](https://vimregex.com/)
Suppose you want to replace all occurrences of vi with VIM. This can be easily done with
s/vi/VIM/g

```
s/vi/VIM/g
```

If you've tried this example then you, no doubt, noticed that VIM replaced all occurrences of vi even if it's a part of the word (e.g. navigator). If we want to be more specific and replace only whole words vi then we need to correct our pattern. We may rewrite it by putting spaces around vi:
s: vi : VIM :g

```
s: vi : VIM :g
```

But it will still miss vi followed by the punctuation or at the end of the line/file. The right way is to put special word boundary symbols "\<" and "\>" around vi.

```
\<
```


```
\>
```

s:\<vi\>:VIM:g

```
s:\<vi\>:VIM:g
```

The beginning and the end of the line have their own special anchors - "^" and "$", respectively. So, for all vi only at the start of the line:

```
^
```


```
$
```

s:^vi\>:VIM:

```
s:^vi\>:VIM:
```

To match the lines where vi is the only word:
s:^vi$:VIM:

```
s:^vi$:VIM:
```

Now suppose you want to replace not only all vi but also Vi and VI. There are several ways to do this:
- probably the simplest way is to put "i" - ignore case in 
          a pattern %s:vi:VIM:gi 

- define a class of characters. This is a sequence of characters enclosed 
          by square brackets "[" and "]". It matches any character 
          from this set. So :%s:[Vv]i:VIM: will match vi 
          and Vi. More on character ranges in the 
          following [section](https://vimregex.com/#ranges). 


```
%s:vi:VIM:gi
```


```
:%s:[Vv]i:VIM:
```

[section](https://vimregex.com/#ranges)

## [4.2 "Escaped" characters or metacharacters](https://vimregex.com/)
[4.2 "Escaped" characters or metacharacters](https://vimregex.com/)
So far our pattern strings were constructed from normal or literal text characters. The power of regexps is in the use of metacharacters. These are types of characters which have special meaning inside the search pattern. With a few exceptions these metacharacters are distinguished by a "magic" backslash in front of them. The table below lists some common VIM metacharacters.
So, to match a date like 09/01/2000 you can use (assuming you don't use "/" as a separator in the S&R)
\d\d/\d\d/\d\d\d\d

```
\d\d/\d\d/\d\d\d\d
```

To match 6 letter word starting with a capital letter
\u\w\w\w\w\w

```
\u\w\w\w\w\w
```

Obviously, it is not very convenient to write \w for any character in the pattern - what if you don't know how many letters in your word? This can be helped by introducing so-called quantifiers.

```
\w
```

[4.3 Quantifiers, Greedy and Non-Greedy](https://vimregex.com/)
Using quantifiers you can set how many times certain part of you pattern should repeat by putting the following after your pattern:
Now it's much easier to define a pattern that matches a word of any length \u\w\+.

```
\u\w\+
```

These quantifiers are greedy - that is your pattern will try to match as much text as possible. Sometimes it presents a problem. Let's consider a typical example - define a pattern to match delimited text, i.e. text enclosed in quotes, brackets, etc. Since we don't know what kind of text is inside the quotes we'll use
/".*"/

```
/".*"/
```

But this pattern will match everything between the first " and the last " in the following line:
this file is normally "$VIM/.gvimrc". You can check this with ":version".

```
this file is normally "$VIM/.gvimrc". You can check this with ":version".
```

This problem can be resolved by using non-greedy quantifiers:
Let's use \{-} in place of * in our pattern. So, now ".\{-}" will match the first quoted text:

```
\{-}
```


```
*
```


```
".\{-}"
```

this file is normally "$VIM/gvimrc". You can check this with ":version".

```
this file is normally "$VIM/gvimrc". You can check this with ":version".
```

.\{-} pattern is not without surprises. Look what will happen to the following text after we apply:

```
.\{-}
```

:s:.\{-}:_:g

```
:s:.\{-}:_:g
```

Before:
n and m are decimal numbers between

```
n and m are decimal numbers between
```

After:
_n_ _a_n_d_ _m_ _a_r_e_ _d_e_c_i_m_a_l_ _n_u_m_b_e_r_s_ _b_e_t_w_e_e_n_

```
_n_ _a_n_d_ _m_ _a_r_e_ _d_e_c_i_m_a_l_ _n_u_m_b_e_r_s_ _b_e_t_w_e_e_n_
```

"As few as possible" applied here means zero character replacements. However match does occur between characters! To explain this behavior I quote Bram himself:
Matching zero characters is still a match. Thus it will replace zero characters with a "_". And then go on to the next position, where it will match again.
It's true that using "\{-}" is mostly useless. It works this way to be consistent with "*", which also matches zero characters. There are more useless ones: "x\{-1,}" always matches one x. You could just use "x". More useful is something like "x\{70}". The others are just consistent behavior: ..., "x\{-3,}", "x\{-2,}", "x\{-1,}.
- Bram
But what if we want to match only the second occurrence of quoted text? Or we want to replace only a part of the quoted text keeping the rest untouched? We will need grouping and backreferences. But before let's talk more about character ranges.

## [4.4 Character ranges](https://vimregex.com/)
[4.4 Character ranges](https://vimregex.com/)
Typical character ranges:
[012345] will match any of the numbers inside the brackets. The same range can be written as [0-5], where dash indicates a range of characters in ASCII order. Likewise, we can define the range for all lowercase letters: [a-z], for all letters: [a-zA-Z], letters and digits: [0-9a-zA-Z] etc. Depending on your system locale you can define range which will include characters like à, Ö, ß and other non ASCII characters.

```
[012345]
```


```
[0-5]
```


```
[a-z]
```


```
[a-zA-Z]
```


```
[0-9a-zA-Z]
```

Note that the range represents just one character in the search pattern, that is [0123] and 0123 are not the same. Likewise the order (with a few exceptions) is not important: [3210] and [0123] are the same character ranges, while 0123 and 3210 are two different patterns. Watch what happens when we apply

```
[0123]
```


```
0123
```


```
[3210]
```


```
[0123]
```


```
0123
```


```
3210
```

s:[65]:Dig:g

```
s:[65]:Dig:g
```

to the following text:
Before:
High 65 to 70. Southeast wind around 10

```
High 65 to 70. Southeast wind around 10
```

After:
High DigDig to 70. Southeast wind around 10

```
High DigDig to 70. Southeast wind around 10
```

and now:
s:65:Dig:g

```
s:65:Dig:g
```

Before:
High 65 to 70. Southeast wind around 10

```
High 65 to 70. Southeast wind around 10
```

After:
High Dig to 70. Southeast wind around 10

```
High Dig to 70. Southeast wind around 10
```

Sometimes it's easier to define the characters you don't want to match. This is done by putting a negation sign "^" (caret) as a first character of the range

```
"^"
```

/[^A-Z]/

```
[^A-Z]
```

- will match any character except capital letters. We can now rewrite our pattern for quoted text using
/"[^"]\+"/

```
/"[^"]\+"
```

Note: inside the [ ] all metacharacters behave like ordinary characters. If you want to include "-" (dash) in your range put it first
/[-0-9]/

```
/[-0-9]/
```

- will match all digits and -. "^" will lose its special meaning if it's not the first character in the range.
Now, let's have some real life example. Suppose you want to run a grammar check on your file and find all places where new sentence does not start with a capital letter. The pattern that will catch this:
\.\s\+[a-z]

```
\.\s\+[a-z]
```

- a period followed by one or more blanks and a lowercase word. We know how to find an error, now let's see how we can correct it. To do this we need some ways to remember our matched pattern and recall it later. That is exactly what backreferences are for.

## [4.5 Grouping and Backreferences](https://vimregex.com/)
[4.5 Grouping and Backreferences](https://vimregex.com/)
You can group parts of the pattern expression enclosing them with "\(" and "\)" and refer to them inside the replacement pattern by their special number \1, \2 ... \9. Typical example is swapping first two words of the line:

```
\(
```


```
\)
```


```
\1, \2 ... \9
```


```
s:\(\w\+\)\(\s\+\)\(\w\+\):\3\2\1:
```

where \1 holds the first word, \2 - any number of spaces or tabs in between and \3 - the second word. How to decide what number holds what pair of \(\) ? - count opening "\(" from the left.

```
\1
```


```
\2
```


```
\3
```


```
\(\)
```


```
\(
```

## Replacement Part of :substitute
Replacement part of the S&R has its own special characters which we are going to use to fix grammar:
Now the full S&R to correct non-capital words at the beginning of the sentences looks like
s:\([.!?]\)\s\+\([a-z]\):\1  \u\2:g

```
s:\([.!?]\)\s\+\([a-z]\):\1  \u\2:g
```

We have corrected our grammar and as an extra job we replaced variable number of spaces between punctuation and the first letter of the next sentence with exactly two spaces.

## [4.6 Alternations](https://vimregex.com/)
[4.6 Alternations](https://vimregex.com/)
Using "\|" you can combine several expressions into one which matches any of its components. The first one matched will be used.

```
\|
```

\(Date:\|Subject:\|From:\)\(\s.*\)

```
\(Date:\|Subject:\|From:\)\(\s.*\)
```

Tip 3: Quick mapping to put \(\) in your pattern string
cmap ;\ \(\)<Left><Left>

```
cmap ;\ \(\)<Left><Left>
```

## [4.7 Regexp Operator Precedence](https://vimregex.com/)
[4.7 Regexp Operator Precedence](https://vimregex.com/)
As in arithmetic expressions, regular expressions are executed in a certain order of precedence. Here the table of precedence, from highest to lowest:

[V. Global Command](https://vimregex.com/)

## [5.1 Global search and execution](https://vimregex.com/)
[5.1 Global search and execution](https://vimregex.com/)
I want to introduce another quite useful and powerful Vim command which we're going to use later
The global commands work by first scanning through the [range] of of the lines and marking each line where a match occurs. In a second scan the [cmd] is executed for each marked line with its line number prepended. If a line is changed or deleted its mark disappears. The default for the [range] is the whole file.
Note: Ex commands are all commands you are entering on the Vim command line like :s[ubstitute], :co[py] , :d[elete], :w[rite] etc. Non-Ex commands (normal mode commands) can be also executed via

```
:s[ubstitute], :co[py] , :d[elete], :w[rite]
```

:norm[al]non-ex command

```
:norm[al]non-ex command
```

mechanism.

## [5.2 Examples](https://vimregex.com/)
[5.2 Examples](https://vimregex.com/)
Some examples of :global usage:

```
:global
```

:g/^$/ d

```
:g/^$/ d
```

- delete all empty lines in a file
:g/^$/,/./-j

```
:g/^$/,/./-j
```

- reduce multiple blank lines to a single blank
:10,20g/^/ mo 10

```
:10,20g/^/ mo 10
```

- reverse the order of the lines starting from the line 10 up to the line 20.
Here is a modified example from [Walter Zintz vi tutorial](http://www.networkcomputing.com/unixworld/tutorial/009/009.html):
:'a,'b g/^Error/ . w >> errors.txt

```
:'a,'b g/^Error/ . w >> errors.txt
```

- in the text block marked by 'a and 'b find all the lines starting with Error and copy (append) them to "errors.txt" file. Note: . (current line address) in front of the w is very important, omitting it will cause :write to write the whole file to "errors.txt" for every Error line found.

```
'a
```


```
'b
```


```
w
```


```
:write
```

You can give multiple commands after :global using "|" as a separator. If you want to use "|' in an argument, precede it with "\'. Another example from Zintz tutorial:

```
:global
```

:g/^Error:/ copy $ | s /Error/copy of the error/

```
:g/^Error:/ copy $ | s /Error/copy of the error/
```

- will copy all Error line to the end of the file and then make a substitution in the copied line. Without giving the line address :s will operate on the current line, which is the newly copied line.

```
:s
```

:g/^Error:/ s /Error/copy of the error/ | copy $

```
:g/^Error:/ s /Error/copy of the error/ | copy $
```

- here the order is reversed: first modify the string then copy to the end.

[VI. Examples](https://vimregex.com/)

## [6.1 Tips and Techniques](https://vimregex.com/)
[6.1 Tips and Techniques](https://vimregex.com/)
A collection of some useful S&R tips:
(1) sent by Antonio Colombo:
"a simple regexp I use quite often to clean up a text: it drops the blanks at the end of the line:"
s:\s*$::

```
s:\s*$::
```

or (to avoid acting on all lines):
s:\s\+$::

```
s:\s\+$::
```

## [6.2 Creating outline](https://vimregex.com/)
[6.2 Creating outline](https://vimregex.com/)
For this example you need to know a bit of HTML. We want to make a table of contents out of h1 and h2 headings, which I will call majors and minors. HTML heading h1 is a text enclosed by <h1> tags as in <h1>Heading</h1>.

```
h1
```


```
h2
```


```
h1
```


```
<h1>
```


```
<h1>Heading</h1>
```

(1) First let's make named anchors in all headings, i.e. put <h1><a name="anchor">Heading</a></h1> around all headings. The "anchor" is a unique identifier of this particular place in HTML document. The following S&R does exactly this:

```
<h1><a name="anchor">Heading</a></h1>
```


```
"anchor
```

:s:\(<h[12]>\)\(.*\s\+\([-a-zA-Z]\+\)\)\s*\(</h[12]>\):\1<a name="\3">\2</a>\4:

```
:s:\(<h[12]>\)\(.*\s\+\([-a-zA-Z]\+\)\)\s*\(</h[12]>\):\1<a name="\3">\2</a>\4:
```

Explanation: the first pair of \(\) saves the opening tag (h1 or h2) to the \1, the second pair saves all heading text before the closing tag, the third pair saves the last word in the heading which we will later use for "anchor" and the last pair saves the closing tag. The replacement is quite obvious - we just reconstruct a new "named" heading using \1-\4 and link tag <a>.

```
\(\)
```


```
h1
```


```
h2
```


```
\1
```


```
\1-\4
```


```
<a>.
```

(2) Now let's copy all headings to one place:
:%g/<h[12]>/ t$

```
:%g/<h[12]>/ t$
```

This command searches our file for the lines starting with <h1> or <h2> and copies them to the end of the file. Now we have a bunch of lines like:

```
<h1>
```


```
<h2>
```

<h1><a name="anchor1">Heading1></a></h1> <h2><a name="anchor2">Heading2></a></h2> <h2><a name="anchor3">Heading3></a></h2> .......................... <h1><a name="anchorN">HeadingN></a></h1>

```
<h1><a name="anchor1">Heading1></a></h1> <h2><a name="anchor2">Heading2></a></h2> <h2><a name="anchor3">Heading3></a></h2> .......................... <h1><a name="anchorN">HeadingN></a></h1>
```

First, we want to convert all name=" to href="# in order to link table entries to their respective places in the text:

```
name="
```


```
href="#
```

s:name=":href="#:

```
s:name=":href="#:
```

Second, we want our h1 entries look different from h2. Let's define CSS classes "majorhead" and "minorhead" and do the following:

```
h1
```


```
h2
```

g/<h1>/ s:<a:& class="majorhead": g/<h2>/ s:<a:& class="minorhead":

```
g/<h1>/ s:<a:& class="majorhead": g/<h2>/ s:<a:& class="minorhead":
```

Now our entries look like:
<h1><a class="majorhead" name="anchor1">Heading1></a></h1> <h2><a class="minorhead" name="anchor2">Heading2></a></h2>

```
<h1><a class="majorhead" name="anchor1">Heading1></a></h1> <h2><a class="minorhead" name="anchor2">Heading2></a></h2>
```

We no longer need h1 and h2 tags:

```
h1
```


```
h2
```

s:<h[21]>::

```
s:<h[21]>::
```

and replace closing tags with breaklines <br>

```
<br>
```

s:/h[21]:br:

```
s:/h[21]:br:
```

<a class="majorhead" name="anchor1">Heading1></a><br> <a class="minorhead" name="anchor2">Heading2></a><br>

```
<a class="majorhead" name="anchor1">Heading1></a><br> <a class="minorhead" name="anchor2">Heading2></a><br>
```

## [6.3 Working with Tables](https://vimregex.com/)
[6.3 Working with Tables](https://vimregex.com/)
Quite often you have to work with a text organized in tables/columns. Consider, for example, the following text
Suppose we want to change all "Europe" cells in the third column to "Asia":
:%s:\(\(\w\+\s\+\)\{2}\)Europe:\1Asia:

```
:%s:\(\(\w\+\s\+\)\{2}\)Europe:\1Asia:
```

To swap the first and the last columns:
:%s:\(\w\+\)\(.*\s\+\)\(\w\+\)$:\3\2\1:

```
:%s:\(\w\+\)\(.*\s\+\)\(\w\+\)$:\3\2\1:
```

To be continued...

# [VII. Other Regexp Flavors](https://vimregex.com/)
[VII. Other Regexp Flavors](https://vimregex.com/)
Here I would like to compare Vim's regexp implementation with others, in particular, Perl's. You can't talk about regular expressions without mentioning Perl.
(with a help from [Steve Kirkendall](mailto:skirkendall@uswest.net)) The main differences between Perl and Vim are:
-  Perl doesn't require backslashes before most of its operators. Personally, 
          I think it makes regexps more readable - the less backlashes are there 
          the better.
- Perl allows you to convert any quantifier into a non-greedy version 
          by adding an extra ? after it. So *? is a non-greedy *.
- Perl supports a lots of weird options that can be appended to the 
          regexp, or even embedded in it.
-  You can also embed variable names in a Perl regular expression. Perl 
          replaces the name with its value; this is called "variable interpolation".

# [VIII. Links](https://vimregex.com/)
[VIII. Links](https://vimregex.com/)
Read VIM documentation about pattern and searching. To get this type ":help pattern" in VIM normal mode.
There are currently two books on the market that deal with VIM regular expressions:
- ["Learning the 
          vi Editor"](http://www.oreilly.com/catalog/vi6/) by Linda Lamb and Arnold Robbins.
- "[vi Improved - VIM](http://www.oualline.com/)" by Steve Oualline
["Learning the 
          vi Editor"](http://www.oreilly.com/catalog/vi6/)
[vi Improved - VIM](http://www.oualline.com/)
Definitive reference on regular expressions is Jeffrey Friedl's ["Mastering Regular Expressions"](http://www.oreilly.com/catalog/regex/chapter/ch04.html) published by O'Reilly & Associates, but it mostly deals with Perl regular expressions. O'Reilly has one of the book chapters available online.
[Oleg 
			 Raisky](mailto:volontir at yahoo dot com)



# Official Vim Regex Reference (pattern.txt)
Source: https://raw.githubusercontent.com/vim/vim/master/runtime/doc/pattern.txt
---

*pattern.txt*	For Vim version 9.2.  Last change: 2026 Feb 14


		  VIM REFERENCE MANUAL	  by Bram Moolenaar


Patterns and search commands				*pattern-searches*

The very basics can be found in section |03.9| of the user manual.  A few more
explanations are in chapter 27 |usr_27.txt|.

1. Search commands		|search-commands|
2. The definition of a pattern	|search-pattern|
3. Magic			|/magic|
4. Overview of pattern items	|pattern-overview|
5. Multi items			|pattern-multi-items|
6. Ordinary atoms		|pattern-atoms|
7. Ignoring case in a pattern	|/ignorecase|
8. Composing characters		|patterns-composing|
9. Compare with Perl patterns	|perl-patterns|
10. Highlighting matches	|match-highlight|
11. Fuzzy matching		|fuzzy-matching|

==============================================================================
1. Search commands				*search-commands*

							*/*
/{pattern}[/]<CR>	Search forward for the [count]'th occurrence of
			{pattern} |exclusive|.

/{pattern}/{offset}<CR>	Search forward for the [count]'th occurrence of
			{pattern} and go |{offset}| lines up or down.
			|linewise|.

							*/<CR>*
/<CR>			Search forward for the [count]'th occurrence of the
			latest used pattern |last-pattern| with latest used
			|{offset}|.

//{offset}<CR>		Search forward for the [count]'th occurrence of the
			latest used pattern |last-pattern| with new
			|{offset}|.  If {offset} is empty no offset is used.

							*?*
?{pattern}[?]<CR>	Search backward for the [count]'th previous
			occurrence of {pattern} |exclusive|.

?{pattern}?{offset}<CR>	Search backward for the [count]'th previous
			occurrence of {pattern} and go |{offset}| lines up or
			down |linewise|.

							*?<CR>*
?<CR>			Search backward for the [count]'th occurrence of the
			latest used pattern |last-pattern| with latest used
			|{offset}|.

??{offset}<CR>		Search backward for the [count]'th occurrence of the
			latest used pattern |last-pattern| with new
			|{offset}|.  If {offset} is empty no offset is used.

							*n*
n			Repeat the latest "/" or "?" [count] times.
			If the cursor doesn't move the search is repeated with
			count + 1.
			|last-pattern|

							*N*
N			Repeat the latest "/" or "?" [count] times in
			opposite direction. |last-pattern|

							*star* *E348* *E349*
*			Search forward for the [count]'th occurrence of the
			word nearest to the cursor.  The word used for the
			search is the first of:
				1. the keyword under the cursor 'iskeyword'
				2. the first keyword after the cursor, in the
				   current line
				3. the non-blank word under the cursor
				4. the first non-blank word after the cursor,
				   in the current line
			Only whole keywords are searched for, like with the
			command "/\<keyword\>".  |exclusive|
			'ignorecase' is used, 'smartcase' is not.

							*#*
#			Same as "*", but search backward.  The pound sign
			(character 163) also works.  If the "#" key works as
			backspace, try using "stty erase <BS>" before starting
			Vim (<BS> is CTRL-H or a real backspace).

							*gstar*
g*			Like "*", but don't put "\<" and "\>" around the word.
			This makes the search also find matches that are not a
			whole word.

							*g#*
g#			Like "#", but don't put "\<" and "\>" around the word.
			This makes the search also find matches that are not a
			whole word.

							*gd*
gd			Goto local Declaration.  When the cursor is on a local
			variable, this command will jump to its declaration.
			This was made to work for C code, in other languages
			it may not work well.
			First Vim searches for the start of the current
			function, just like "[[".  If it is not found the
			search stops in line 1.  If it is found, Vim goes back
			until a blank line is found.  From this position Vim
			searches for the keyword under the cursor, like with
			"*", but lines that look like a comment are ignored
			(see 'comments' option).
			Note that this is not guaranteed to work, Vim does not
			really check the syntax, it only searches for a match
			with the keyword.  If included files also need to be
			searched use the commands listed in |include-search|.
			After this command |n| searches forward for the next
			match (not backward).

							*gD*
gD			Goto global Declaration.  When the cursor is on a
			global variable that is defined in the file, this
			command will jump to its declaration.  This works just
			like "gd", except that the search for the keyword
			always starts in line 1.

							*1gd*
1gd			Like "gd", but ignore matches inside a {} block that
			ends before the cursor position.

							*1gD*
1gD			Like "gD", but ignore matches inside a {} block that
			ends before the cursor position.

							*CTRL-C*
CTRL-C			Interrupt current (search) command.  Use CTRL-Break on
			MS-Windows |dos-CTRL-Break|.
			In Normal mode, any pending command is aborted.
			When Vim was started with output redirected and there
			are no changed buffers CTRL-C exits Vim.  That is to
			help users who use "vim file | grep word" and don't
			know how to get out (blindly typing :qa<CR> would
			work).
			If a popup with a |popup-filter| is open, the popup
			will be closed.

							*:noh* *:nohlsearch*
:noh[lsearch]		Stop the highlighting for the 'hlsearch' option.  It
			is automatically turned back on when using a search
			command, or setting the 'hlsearch' option.
			This command doesn't work in an autocommand, because
			the highlighting state is saved and restored when
			executing autocommands |autocmd-searchpat|.
			Same thing for when invoking a user function.


While typing the search pattern the current match will be shown if the
'incsearch' option is on.  Remember that you still have to finish the search
command with <CR> to actually position the cursor at the displayed match.  Or
use <Esc> to abandon the search.

							*nohlsearch-auto*
All matches for the last used search pattern will be highlighted if you set
the 'hlsearch' option.  This can be suspended with the |:nohlsearch| command
or auto suspended with nohlsearch plugin.  See |nohlsearch-install|.


When 'shortmess' does not include the "S" flag, Vim will automatically show an
index, on which the cursor is.  This can look like this: >

  [1/5]		Cursor is on first of 5 matches.
  [1/>99]	Cursor is on first of more than 99 matches.
  [>99/>99]	Cursor is after 99 match of more than 99 matches.
  [?/??]	Unknown how many matches exists, generating the
		statistics was aborted because of search timeout.

Note: the count does not take offset into account.

When no match is found you get the error: *E486* Pattern not found
Note that for the `:global` command, when used in legacy script, you get a
normal message "Pattern not found", for Vi compatibility.
In |Vim9| script you get E486 for "pattern not found" or *E538* when the pattern
matches in every line with `:vglobal`.
For the |:s| command the "e" flag can be used to avoid the error message
|:s_flags|.

					*search-options*
The following options affect how a search is performed in Vim:
    'hlsearch'		highlight matches
    'ignorecase'	ignore case when searching
    'imsearch'		use |IME| when entering the search pattern
    'incsearch'		show matches incrementally as the pattern is typed
    'maxsearchcount'	maximum number for the search count |shm-S|
    'shortmess'		suppress messages |shm-s|; show search count |shm-S|
    'smartcase'		override 'ignorecase' if pattern contains uppercase
    'wrapscan'		continue searching from the start of the file

					*search-offset* *{offset}*
These commands search for the specified pattern.  With "/" and "?" an
additional offset may be given.  There are two types of offsets: line offsets
and character offsets.

The offset gives the cursor position relative to the found match:
    [num]	[num] lines downwards, in column 1
    +[num]	[num] lines downwards, in column 1
    -[num]	[num] lines upwards, in column 1
    e[+num]	[num] characters to the right of the end of the match
    e[-num]	[num] characters to the left of the end of the match
    s[+num]	[num] characters to the right of the start of the match
    s[-num]	[num] characters to the left of the start of the match
    b[+num]	[num] identical to s[+num] above (mnemonic: begin)
    b[-num]	[num] identical to s[-num] above (mnemonic: begin)
    ;{pattern}  perform another search, see |//;|

If a '-' or '+' is given but [num] is omitted, a count of one will be used.
When including an offset with 'e', the search becomes inclusive (the
character the cursor lands on is included in operations).

Examples:

pattern			cursor position	~
/test/+1		one line below "test", in column 1
/test/e			on the last t of "test"
/test/s+2		on the 's' of "test"
/test/b-3		three characters before "test"

If one of these commands is used after an operator, the characters between
the cursor position before and after the search is affected.  However, if a
line offset is given, the whole lines between the two cursor positions are
affected.

An example of how to search for matches with a pattern and change the match
with another word: >
	/foo<CR>	find "foo"
	c//e<CR>	change until end of match
	bar<Esc>	type replacement
	//<CR>		go to start of next match
	c//e<CR>	change until end of match
	beep<Esc>	type another replacement
			etc.
<
							*//;* *E386*
A very special offset is ';' followed by another search command.  For example: >

   /test 1/;/test
   /test.*/+1;?ing?

The first one first finds the next occurrence of "test 1", and then the first
occurrence of "test" after that.

This is like executing two search commands after each other, except that:
- It can be used as a single motion command after an operator.
- The direction for a following "n" or "N" command comes from the first
  search command.
- When an error occurs the cursor is not moved at all.

							*last-pattern*
The last used pattern and offset are remembered.  They can be used to repeat
the search, possibly in another direction or with another count.  Note that
two patterns are remembered: One for "normal" search commands and one for the
substitute command ":s".  Each time an empty pattern is given, the previously
used pattern is used.  However, if there is no previous search command, a
previous substitute pattern is used, if possible.

The 'magic' option sticks with the last used pattern.  If you change 'magic',
this will not change how the last used pattern will be interpreted.
The 'ignorecase' option does not do this.  When 'ignorecase' is changed, it
will result in the pattern to match other text.

All matches for the last used search pattern will be highlighted if you set
the 'hlsearch' option.

To clear the last used search pattern: >
	:let @/ = ""
This will not set the pattern to an empty string, because that would match
everywhere.  The pattern is really cleared, like when starting Vim.

The search usually skips matches that don't move the cursor.  Whether the next
match is found at the next character or after the skipped match depends on the
'c' flag in 'cpoptions'.  See |cpo-c|.
	   with 'c' flag:   "/..." advances 1 to 3 characters
	without 'c' flag:   "/..." advances 1 character
The unpredictability with the 'c' flag is caused by starting the search in the
first column, skipping matches until one is found past the cursor position.

When searching backwards, searching starts at the start of the line, using the
'c' flag in 'cpoptions' as described above.  Then the last match before the
cursor position is used.

In Vi the ":tag" command sets the last search pattern when the tag is searched
for.  In Vim this is not done, the previous search pattern is still
remembered, unless the 't' flag is present in 'cpoptions'.  The search pattern
is always put in the search history.

If the 'wrapscan' option is on (which is the default), searches wrap around
the end of the buffer.  If 'wrapscan' is not set, the backward search stops
at the beginning and the forward search stops at the end of the buffer.  If
'wrapscan' is set and the pattern was not found the error message "pattern
not found" is given, and the cursor will not be moved.  If 'wrapscan' is not
set the message becomes "search hit BOTTOM without match" when searching
forward, or "search hit TOP without match" when searching backward.  If
wrapscan is set and the search wraps around the end of the file the message
"search hit TOP, continuing at BOTTOM" or "search hit BOTTOM, continuing at
TOP" is given when searching backwards or forwards respectively.  This can be
switched off by setting the 's' flag in the 'shortmess' option.  The highlight
method 'w' is used for this message (default: standout).

							*search-range*
You can limit the search command "/" to a certain range of lines by including
\%>l items.  For example, to match the word "limit" below line 199 and above
line 300: >
	/\%>199l\%<300llimit
Also see |/\%>l|.

Another way is to use the ":substitute" command with the 'c' flag.  Example: >
   :.,300s/Pattern//gc
This command will search from the cursor position until line 300 for
"Pattern".  At the match, you will be asked to type a character.  Type 'q' to
stop at this match, type 'n' to find the next match.

The "*", "#", "g*" and "g#" commands look for a word near the cursor in this
order, the first one that is found is used:
- The keyword currently under the cursor.
- The first keyword to the right of the cursor, in the same line.
- The WORD currently under the cursor.
- The first WORD to the right of the cursor, in the same line.
The keyword may only contain letters and characters in 'iskeyword'.
The WORD may contain any non-blanks (<Tab>s and/or <Space>s).
Note that if you type with ten fingers, the characters are easy to remember:
the "#" is under your left hand middle finger (search to the left and up) and
the "*" is under your right hand middle finger (search to the right and down).
(this depends on your keyboard layout though).

								*E956*
In very rare cases a regular expression is used recursively.  This can happen
when executing a pattern takes a long time and when checking for messages on
channels a callback is invoked that also uses a pattern or an autocommand is
triggered.  In most cases this should be fine, but if a pattern is in use when
it's used again it fails.  Usually this means there is something wrong with
the pattern.

==============================================================================
2. The definition of a pattern		*search-pattern* *pattern* *[pattern]*
					*regular-expression* *regexp* *Pattern*
					*E383* *E476*

For starters, read chapter 27 of the user manual |usr_27.txt|.

						*/bar* */\bar* */pattern*
1. A pattern is one or more branches, separated by "\|".  It matches anything
   that matches one of the branches.  Example: "foo\|beep" matches "foo" and
   matches "beep".  If more than one branch matches, the first one is used.

   pattern ::=	    branch
		or  branch \| branch
		or  branch \| branch \| branch
		etc.

						*/branch* */\&*
2. A branch is one or more concats, separated by "\&".  It matches the last
   concat, but only if all the preceding concats also match at the same
   position.  Examples:
	"foobeep\&..." matches "foo" in "foobeep".
	".*Peter\&.*Bob" matches in a line containing both "Peter" and "Bob"

   branch ::=	    concat
		or  concat \& concat
		or  concat \& concat \& concat
		etc.

						*/concat*
3. A concat is one or more pieces, concatenated.  It matches a match for the
   first piece, followed by a match for the second piece, etc.  Example:
   "f[0-9]b", first matches "f", then a digit and then "b".

   concat  ::=	    piece
		or  piece piece
		or  piece piece piece
		etc.

						*/piece*
4. A piece is an atom, possibly followed by a multi, an indication of how many
   times the atom can be matched.  Example: "a*" matches any sequence of "a"
   characters: "", "a", "aa", etc.  See |/multi|.

   piece   ::=	    atom
		or  atom  multi

						*/atom*
5. An atom can be one of a long list of items.  Many atoms match one character
   in the text.  It is often an ordinary character or a character class.
   Parentheses can be used to make a pattern into an atom.  The "\z(\)"
   construct is only for syntax highlighting.

   atom    ::=	    ordinary-atom		|/ordinary-atom|
		or  \( pattern \)		|/\(|
		or  \%( pattern \)		|/\%(|
		or  \z( pattern \)		|/\z(|


				*/\%#=* *two-engines* *NFA*
Vim includes two regexp engines:
1. An old, backtracking engine that supports everything.
2. A new, NFA engine that works much faster on some patterns, possibly slower
   on some patterns.
								 *E1281*
Vim will automatically select the right engine for you.  However, if you run
into a problem or want to specifically select one engine or the other, you can
prepend one of the following to the pattern:

	\%#=0	Force automatic selection.  Only has an effect when
	        'regexpengine' has been set to a non-zero value.
	\%#=1	Force using the old engine.
	\%#=2	Force using the NFA engine.

You can also use the 'regexpengine' option to change the default.

			 *E864* *E868* *E874* *E875* *E876* *E877* *E878*
If selecting the NFA engine and it runs into something that is not implemented
the pattern will not match.  This is only useful when debugging Vim.

==============================================================================
3. Magic							*/magic*

Some characters in the pattern, such as letters, are taken literally.  They
match exactly the same character in the text.  When preceded with a backslash
however, these characters may get a special meaning.  For example, "a" matches
the letter "a", while "\a" matches any alphabetic character.

Other characters have a special meaning without a backslash.  They need to be
preceded with a backslash to match literally.  For example "." matches any
character while "\." matches a dot.

If a character is taken literally or not depends on the 'magic' option and the
items in the pattern mentioned next.  The 'magic' option should always be set,
but it can be switched off for Vi compatibility.  We mention the effect of
'nomagic' here for completeness, but we recommend against using that.
							*/\m* */\M*
Use of "\m" makes the pattern after it be interpreted as if 'magic' is set,
ignoring the actual value of the 'magic' option.
Use of "\M" makes the pattern after it be interpreted as if 'nomagic' is used.
							*/\v* */\V*
Use of "\v" means that after it, all ASCII characters except '0'-'9', 'a'-'z',
'A'-'Z' and '_' have special meaning: "very magic"

Use of "\V" means that after it, only a backslash and the terminating
character (usually / or ?) have special meaning: "very nomagic"

Examples:
after:	  \v	   \m	    \M	     \V		matches ~
		'magic' 'nomagic'
	  a	   a	    a	     a		literal 'a'
	  \a	   \a	    \a	     \a		any alphabetic character
	  .	   .	    \.	     \.		any character
	  \.	   \.	    .	     .		literal dot
	  $	   $	    $	     \$		end-of-line
	  *	   *	    \*	     \*		any number of the previous atom
	  ~	   ~	    \~	     \~		latest substitute string
	  ()	   \(\)     \(\)     \(\)	group as an atom
	  |	   \|	    \|	     \|		nothing: separates alternatives
	  \\	   \\	    \\	     \\		literal backslash
	  \{	   {	    {	     {		literal curly brace

{only Vim supports \m, \M, \v and \V}

If you want to you can make a pattern immune to the 'magic' option being set
or not by putting "\m" or "\M" at the start of the pattern.

==============================================================================
4. Overview of pattern items				*pattern-overview*
						*E865* *E866* *E867* *E869*

Overview of multi items.				*/multi* *E61* *E62*
More explanation and examples below, follow the links.		*E64* *E871*

	  multi ~
     'magic' 'nomagic'	matches of the preceding atom ~
|/star|	*	\*	0 or more	as many as possible
|/\+|	\+	\+	1 or more	as many as possible
|/\=|	\=	\=	0 or 1		as many as possible
|/\?|	\?	\?	0 or 1		as many as possible

|/\{|	\{n,m}	\{n,m}	n to m		as many as possible
	\{n}	\{n}	n		exactly
	\{n,}	\{n,}	at least n	as many as possible
	\{,m}	\{,m}	0 to m		as many as possible
	\{}	\{}	0 or more	as many as possible (same as *)

|/\{-|	\{-n,m}	\{-n,m}	n to m		as few as possible
	\{-n}	\{-n}	n		exactly
	\{-n,}	\{-n,}	at least n	as few as possible
	\{-,m}	\{-,m}	0 to m		as few as possible
	\{-}	\{-}	0 or more	as few as possible

							*E59*
|/\@>|	\@>	\@>	1, like matching a whole pattern
|/\@=|	\@=	\@=	nothing, requires a match |/zero-width|
|/\@!|	\@!	\@!	nothing, requires NO match |/zero-width|
|/\@<=|	\@<=	\@<=	nothing, requires a match behind |/zero-width|
|/\@<!|	\@<!	\@<!	nothing, requires NO match behind |/zero-width|


Overview of ordinary atoms.				*/ordinary-atom*
More explanation and examples below, follow the links.

      ordinary atom ~
      magic   nomagic	matches ~
|/^|	^	^	start-of-line (at start of pattern) |/zero-width|
|/\^|	\^	\^	literal '^'
|/\_^|	\_^	\_^	start-of-line (used anywhere) |/zero-width|
|/$|	$	$	end-of-line (at end of pattern) |/zero-width|
|/\$|	\$	\$	literal '$'
|/\_$|	\_$	\_$	end-of-line (used anywhere) |/zero-width|
|/.|	.	\.	any single character (not an end-of-line)
|/\_.|	\_.	\_.	any single character or end-of-line
|/\<|	\<	\<	beginning of a word |/zero-width|
|/\>|	\>	\>	end of a word |/zero-width|
|/\zs|	\zs	\zs	anything, sets start of match
|/\ze|	\ze	\ze	anything, sets end of match
|/\%^|	\%^	\%^	beginning of file |/zero-width|		*E71*
|/\%$|	\%$	\%$	end of file |/zero-width|
|/\%V|	\%V	\%V	inside Visual area |/zero-width|
|/\%#|	\%#	\%#	cursor position |/zero-width|
|/\%'m|	\%'m	\%'m	mark m position |/zero-width|
|/\%l|	\%23l	\%23l	in line 23 |/zero-width|
|/\%c|	\%23c	\%23c	in column 23 |/zero-width|



# Official Vim Search & Replace Reference (change.txt)
Source: https://raw.githubusercontent.com/vim/vim/master/runtime/doc/change.txt
---

*change.txt*	For Vim version 9.2.  Last change: 2026 Jun 26


		  VIM REFERENCE MANUAL	  by Bram Moolenaar


This file describes commands that delete or change text.  In this context,
changing text means deleting the text and replacing it with other text using
one command.  You can undo all of these commands.  You can repeat the non-Ex
commands with the "." command.

1. Deleting text		|deleting|
2. Delete and insert		|delete-insert|
3. Simple changes		|simple-change|		*changing*
4. Complex changes		|complex-change|
   4.1 Filter commands		   |filter|
   4.2 Substitute		   |:substitute|
   4.3 Search and replace	   |search-replace|
   4.4 Changing tabs		   |change-tabs|
5. Copying and moving text	|copy-move|
6. Formatting text		|formatting|
7. Sorting text			|sorting|
8. Deduplicating text		|deduplicating|

For inserting text see |insert.txt|.

==============================================================================
1. Deleting text					*deleting* *E470*

["x]<Del>	or					*<Del>* *x* *dl*
["x]x			Delete [count] characters under and after the cursor
			[into register x] (not |linewise|).  Does the same as
			"dl".
			The <Del> key does not take a [count].  Instead, it
			deletes the last character of the count.
			See |:fixdel| if the <Del> key does not do what you
			want.  See 'whichwrap' for deleting a line break
			(join lines).

							*X* *dh*
["x]X			Delete [count] characters before the cursor [into
			register x] (not |linewise|).  Does the same as "dh".
			Also see 'whichwrap'.

							*d*
["x]d{motion}		Delete text that {motion} moves over [into register
			x].  See below for exceptions.

							*dd*
["x]dd			Delete [count] lines [into register x] |linewise|.

							*D*
["x]D			Delete the characters under the cursor until the end
			of the line and [count]-1 more lines [into register
			x]; synonym for "d$".
			(not |linewise|)
			When the '#' flag is in 'cpoptions' the count is
			ignored.

{Visual}["x]x	or					*v_x* *v_d* *v_<Del>*
{Visual}["x]d   or
{Visual}["x]<Del>	Delete the highlighted text [into register x] (for
			{Visual} see |Visual-mode|).

{Visual}["x]CTRL-H   or					*v_CTRL-H* *v_<BS>*
{Visual}["x]<BS>	When in Select mode: Delete the highlighted text [into
			register x].

{Visual}["x]X	or					*v_X* *v_D* *v_b_D*
{Visual}["x]D		Delete the highlighted lines [into register x] (for
			{Visual} see |Visual-mode|).  In Visual block mode,
			"D" deletes the highlighted text plus all text until
			the end of the line.

					*:d* *:de* *:del* *:delete* *:dl* *:dp*
:[range]d[elete] [x]	Delete [range] lines (default: current line) [into
			register x].
			Note these weird abbreviations applicable only to
			legacy Vim script:
			  :dl		delete and list
			  :dell		idem
			  :delel	idem
			  :deletl	idem
			  :deletel	idem
			  :dp		delete and print
			  :dep		idem
			  :delp		idem
			  :delep	idem
			  :deletp	idem
			  :deletep	idem
			Warning: These give |E492| in |Vim9| script and `:dl`
			executes as `:dlist`.

:[range]d[elete] [x] {count}
			Delete {count} lines, starting with [range]
			(default: current line |cmdline-ranges|) [into
			register x].

These commands delete text.  You can repeat them with the `.` command
(except `:d`) and undo them.  Use Visual mode to delete blocks of text.  See
|registers| for an explanation of registers.
							*d-special*
An exception for the d{motion} command: If the motion is not linewise, the
start and end of the motion are not in the same line, and there are only
blanks before the start and there are no non-blanks after the end of the
motion, the delete becomes linewise.  This means that the delete also removes
the line of blanks that you might expect to remain.  Use the |o_v| operator to
force the motion to be characterwise or remove the "z" flag from 'cpoptions'
(see |cpo-z|) to disable this peculiarity.

Trying to delete an empty region of text (e.g., "d0" in the first column)
is an error when 'cpoptions' includes the 'E' flag.

							*J*
J			Join [count] lines, with a minimum of two lines.
			Remove the indent and insert up to two spaces (see
			below).  Fails when on the last line of the buffer.
			If [count] is too big it is reduced to the number of
			lines available.

							*v_J*
{Visual}J		Join the highlighted lines, with a minimum of two
			lines.  Remove the indent and insert up to two spaces
			(see below).

							*gJ*
gJ			Join [count] lines, with a minimum of two lines.
			Don't insert or remove any spaces.

							*v_gJ*
{Visual}gJ		Join the highlighted lines, with a minimum of two
			lines.  Don't insert or remove any spaces.

							*:j* *:join*
:[range]j[oin][!] [flags]
			Join [range] lines.  Same as "J", except with [!]
			the join does not insert or delete any spaces.
			If a [range] has equal start and end values, this
			command does nothing.  The default behavior is to
			join the current line with the line below it.
			See |ex-flags| for [flags].

:[range]j[oin][!] {count} [flags]
			Join {count} lines, starting with [range] (default:
			current line |cmdline-ranges|).  Same as "J", except
			with [!] the join does not insert or delete any
			spaces.
			See |ex-flags| for [flags].

These commands delete the <EOL> between lines.  This has the effect of joining
multiple lines into one line.  You can repeat these commands (except `:j`) and
undo them.

These commands, except "gJ", insert one space in place of the <EOL> unless
there is trailing white space or the next line starts with a ')'.  These
commands, except "gJ", delete any leading white space on the next line.  If
the 'joinspaces' option is on, these commands insert two spaces after a '.',
'!' or '?' (but if 'cpoptions' includes the 'j' flag, they insert two spaces
only after a '.').
The 'B' and 'M' flags in 'formatoptions' change the behavior for inserting
spaces before and after a multibyte character |fo-table|.

The |'[| mark is set at the end of the first line that was joined, |']| at the
end of the resulting line.


==============================================================================
2. Delete and insert				*delete-insert* *replacing*

							*R*
R			Enter Replace mode: Each character you type replaces
			an existing character, starting with the character
			under the cursor.  Repeat the entered text [count]-1
			times.  See |Replace-mode| for more details.

							*gR*
gR			Enter Virtual Replace mode: Each character you type
			replaces existing characters in screen space.  So a
			<Tab> may replace several characters at once.
			Repeat the entered text [count]-1 times.  See
			|Virtual-Replace-mode| for more details.

							*c*
["x]c{motion}		Delete {motion} text [into register x] and start
			insert.  When  'cpoptions' includes the 'E' flag and
			there is no text to delete (e.g., with "cTx" when the
			cursor is just after an 'x'), an error occurs and
			insert mode does not start (this is Vi compatible).
			When  'cpoptions' does not include the 'E' flag, the
			"c" command always starts insert mode, even if there
			is no text to delete.

							*cc*
["x]cc			Delete [count] lines [into register x] and start
			insert |linewise|.  If 'autoindent' is on, preserve
			the indent of the first line.

							*C*
["x]C			Delete from the cursor position to the end of the
			line and [count]-1 more lines [into register x], and
			start insert.  Synonym for c$ (not |linewise|).

							*s*
["x]s			Delete [count] characters [into register x] and start
			insert (s stands for Substitute).  Synonym for "cl"
			(not |linewise|).

							*S*
["x]S			Delete [count] lines [into register x] and start
			insert.  Synonym for "cc" |linewise|.

{Visual}["x]c	or					*v_c* *v_s*
{Visual}["x]s		Delete the highlighted text [into register x] and
			start insert (for {Visual} see |Visual-mode|).

							*v_r*
{Visual}r{char}		Replace all selected characters by {char}.
			CTRL-C will be inserted literally.

							*v_C*
{Visual}["x]C		Delete the highlighted lines [into register x] and
			start insert.  In Visual block mode it works
			differently |v_b_C|.
							*v_S*
{Visual}["x]S		Delete the highlighted lines [into register x] and
			start insert (for {Visual} see |Visual-mode|).
							*v_R*
{Visual}["x]R		Currently just like {Visual}["x]S.  In a next version
			it might work differently.

Notes:
- You can end Insert and Replace mode with <Esc>.
- See the section "Insert and Replace mode" |mode-ins-repl| for the other
  special characters in these modes.
- The effect of [count] takes place after Vim exits Insert or Replace mode.
- When the 'cpoptions' option contains '$' and the change is within one line,
  Vim continues to show the text to be deleted and puts a '$' at the last
  deleted character.

See |registers| for an explanation of registers.

Replace mode is just like Insert mode, except that every character you enter
deletes one character.  If you reach the end of a line, Vim appends any
further characters (just like Insert mode).  In Replace mode, the backspace
key restores the original text (if there was any).  (See section "Insert and
Replace mode" |mode-ins-repl|).

						*cw* *cW*
Special case: When the cursor is in a word, "cw" and "cW" do not include the
white space after a word, they only change up to the end of the word.  This is
because Vim interprets "cw" as change-word, and a word does not include the
following white space.
{Vi: "cw" when on a blank followed by other blanks changes only the first
blank; this is probably a bug, because "dw" deletes all the blanks; use the
'w' flag in 'cpoptions' to make it work like Vi anyway}

If you prefer "cw" to include the space after a word, use this mapping: >
	:map cw dwi
Alternatively use "caw" (see also |aw| and |cpo-z|).

							*:c* *:ch* *:change*
:{range}c[hange][!]	Replace lines of text with some different text.
			Type a line containing only "." to stop replacing.
			Without {range}, this command changes only the current
			line.
			Adding [!] toggles 'autoindent' for the time this
			command is executed.
			This command is not supported in |Vim9| script,
			because it is too easily confused with a variable
			name.

==============================================================================
3. Simple changes					*simple-change*

							*r*
r{char}			Replace the character under the cursor with {char}.
			If {char} is a <CR> or <NL>, a line break replaces the
			character.  To replace with a real <CR>, use CTRL-V
			<CR>.  CTRL-V <NL> replaces with a <Nul>.

			If {char} is CTRL-E or CTRL-Y the character from the
			line below or above is used, just like with |i_CTRL-E|
			and |i_CTRL-Y|.  This also works with a count, thus
			`10r<C-E>` copies 10 characters from the line below.

			If you give a [count], Vim replaces [count] characters
			with [count] {char}s.  When {char} is a <CR> or <NL>,
			however, Vim inserts only one <CR>: "5r<CR>" replaces
			five characters with a single line break.
			When {char} is a <CR> or <NL>, Vim performs
			autoindenting.  This works just like deleting the
			characters that are replaced and then doing
			"i<CR><Esc>".
			{char} can be entered as a digraph |digraph-arg|.
			|:lmap| mappings apply to {char}.  The CTRL-^ command
			in Insert mode can be used to switch this on/off
			|i_CTRL-^|.  See |utf-8-char-arg| about using
			composing characters when 'encoding' is Unicode.

							*gr*
gr{char}		Replace the virtual characters under the cursor with
			{char}.  This replaces in screen space, not file
			space.  See |gR| and |Virtual-Replace-mode| for more
			details.  As with |r| a count may be given.
			{char} can be entered like with |r|, but characters
			that have a special meaning in Insert mode, such as
			most CTRL-keys, cannot be used.

						*digraph-arg*
The argument for Normal mode commands like |r| and |t| is a single character.
When 'cpo' doesn't contain the 'D' flag, this character can also be entered
like |digraphs|.  First type CTRL-K and then the two digraph characters.
{not available when compiled without the |+digraphs| feature}

						*case*
The following commands change the case of letters.  The currently active
|locale| is used.  See |:language|.  The LC_CTYPE value matters here.

							*~*
~			'notildeop' option: Switch case of the character
			under the cursor and move the cursor to the right.
			If a [count] is given, do that many characters.

~{motion}		'tildeop' option: switch case of {motion} text.

							*g~*
g~{motion}		Switch case of {motion} text.

g~g~							*g~g~* *g~~*
g~~			Switch case of current line.

							*v_~*
{Visual}~		Switch case of highlighted text (for {Visual} see
			|Visual-mode|).

							*v_U*
{Visual}U		Make highlighted text uppercase (for {Visual} see
			|Visual-mode|).

							*gU* *uppercase*
gU{motion}		Make {motion} text uppercase.
			Example: >
				:map! <C-F> <Esc>gUiw`]a
<			This works in Insert mode: press CTRL-F to make the
			word before the cursor uppercase.  Handy to type
			words in lowercase and then make them uppercase.


gUgU							*gUgU* *gUU*
gUU			Make current line uppercase.

							*v_u*
{Visual}u		Make highlighted text lowercase (for {Visual} see
			|Visual-mode|).

							*gu* *lowercase*
gu{motion}		Make {motion} text lowercase.

gugu							*gugu* *guu*
guu			Make current line lowercase.

							*g?* *rot13*
g?{motion}		Rot13 encode {motion} text.

							*v_g?*
{Visual}g?		Rot13 encode the highlighted text (for {Visual} see
			|Visual-mode|).

g?g?							*g?g?* *g??*
g??			Rot13 encode current line.

To turn one line into title caps, make every first letter of a word
uppercase: >
	:s/\v<(.)(\w*)/\u\1\L\2/g


Adding and subtracting ~
							*CTRL-A*
CTRL-A			Add [count] to the number or alphabetic character at
			or after the cursor.

							*v_CTRL-A*
{Visual}CTRL-A		Add [count] to the number or alphabetic character in
			the highlighted text.

							*v_g_CTRL-A*
{Visual}g CTRL-A	Add [count] to the number or alphabetic character in
			the highlighted text.  If several lines are
		        highlighted, each one will be incremented by an
			additional [count] (so effectively creating a
			[count] incrementing sequence).
			For Example, if you have this list of numbers:
				1. ~
				1. ~
				1. ~
				1. ~
			Move to the second "1." and Visually select three
			lines, pressing g CTRL-A results in:
				1. ~
				2. ~
				3. ~
				4. ~

							*CTRL-X*
CTRL-X			Subtract [count] from the number or alphabetic
			character at or after the cursor.

							*v_CTRL-X*
{Visual}CTRL-X		Subtract [count] from the number or alphabetic
			character in the highlighted text.

			On MS-Windows, this is mapped to cut Visual text
			|dos-standard-mappings|.  If you want to disable the
			mapping, use this: >
				silent! vunmap <C-X>
<
							*v_g_CTRL-X*
{Visual}g CTRL-X	Subtract [count] from the number or alphabetic
			character in the highlighted text.  If several lines
			are highlighted, each value will be decremented by an
			additional [count] (so effectively creating a [count]
			decrementing sequence).

The CTRL-A and CTRL-X commands can work for:
- signed and unsigned decimal numbers
- unsigned binary, octal and hexadecimal numbers
- alphabetic characters

This depends on the 'nrformats' option:
- When 'nrformats' includes "bin", Vim assumes numbers starting with '0b' or
  '0B' are binary.
- When 'nrformats' includes "octal", Vim considers numbers starting with a '0'
  to be octal, unless the number includes a '8' or '9'.  Other numbers are
  decimal and may have a preceding minus sign.
  If the cursor is on a number, the commands apply to that number; otherwise
  Vim uses the number to the right of the cursor.
- When 'nrformats' includes "hex", Vim assumes numbers starting with '0x' or
  '0X' are hexadecimal.  The case of the rightmost letter in the number
  determines the case of the resulting hexadecimal number.  If there is no
  letter in the current number, Vim uses the previously detected case.
- When 'nrformats' includes "alpha", Vim will change the alphabetic character
  under or after the cursor.  This is useful to make lists with an alphabetic
  index.

For decimals a leading negative sign is considered for incrementing/
decrementing, for binary, octal and hex values, it won't be considered.  To
ignore the sign Visually select the number before using CTRL-A or CTRL-X.

For numbers with leading zeros (including all octal and hexadecimal numbers),
Vim preserves the number of characters in the number when possible.  CTRL-A on
"0077" results in "0100", CTRL-X on "0x100" results in "0x0ff".
There is one exception: When a number that starts with a zero is found not to
be octal (it contains a '8' or '9'), but 'nrformats' does include "octal",
leading zeros are removed to avoid that the result may be recognized as an
octal number.

Note that when 'nrformats' includes "octal", decimal numbers with leading
zeros cause mistakes, because they can be confused with octal numbers.

Note similarly, when 'nrformats' includes both "bin" and "hex", binary numbers
with a leading '0x' or '0X' can be interpreted as hexadecimal rather than
binary since '0b' are valid hexadecimal digits.  CTRL-A on "0x0b11" results in
"0x0b12", not "0x0b100".
When 'nrformats' includes "bin" and doesn't include "hex", CTRL-A on "0b11" in
"0x0b11" results in "0x0b100".

When the number under the cursor is too big to fit into 32 or 64 bit
(depending on how Vim was build), it will be rounded off to the nearest number
that can be represented, and the addition/subtraction is skipped.  E.g. with
64 bit support using CTRL-X on 18446744073709551616 results in
18446744073709551615.  Same for larger numbers, such as 18446744073709551618.

The CTRL-A command is very useful in a macro.  Example: Use the following
steps to make a numbered list.

1. Create the first list entry, make sure it starts with a number.
2. qa	     - start recording into register 'a'
3. Y	     - yank the entry
4. p	     - put a copy of the entry below the first one
5. CTRL-A    - increment the number
6. q	     - stop recording
7. <count>@a - repeat the yank, put and increment <count> times


SHIFTING LINES LEFT OR RIGHT				*shift-left-right*

							*<*
<{motion}		Shift {motion} lines one 'shiftwidth' leftwards.

			If the 'vartabstop' feature is enabled, and the
			'shiftwidth' option is set to zero, the amount of
			indent is calculated at the first non-blank character
			in the line.
							*<<*
<<			Shift [count] lines one 'shiftwidth' leftwards.

							*v_<*
{Visual}[count]<	Shift the highlighted lines [count] 'shiftwidth'
			leftwards (for {Visual} see |Visual-mode|).

							*>*
 >{motion}		Shift {motion} lines one 'shiftwidth' rightwards.

			If the 'vartabstop' feature is enabled, and the
			'shiftwidth' option is set to zero, the amount of
			indent is calculated at the first non-blank character
			in the line.
							*>>*
 >>			Shift [count] lines one 'shiftwidth' rightwards.

							*v_>*
{Visual}[count]>	Shift the highlighted lines [count] 'shiftwidth'
			rightwards (for {Visual} see |Visual-mode|).

							*:<*
:[range]<		Shift [range] lines one 'shiftwidth' left.  Repeat '<'
			for shifting multiple 'shiftwidth's.

:[range]< {count}	Shift {count} lines one 'shiftwidth' left, starting
			with [range] (default current line |cmdline-ranges|).
			Repeat '<' for shifting multiple 'shiftwidth's.

:[range]le[ft] [indent]	left align lines in [range].  Sets the indent in the
			lines to [indent] (default 0).

							*:>*
:[range]> [flags]	Shift [range] lines one 'shiftwidth' right.
			Repeat '>' for shifting multiple 'shiftwidth's.
			See |ex-flags| for [flags].

:[range]> {count} [flags]
			Shift {count} lines one 'shiftwidth' right, starting
			with [range] (default current line |cmdline-ranges|).
			Repeat '>' for shifting multiple 'shiftwidth's.
			See |ex-flags| for [flags].

The ">" and "<" commands are handy for changing the indentation within
programs.  Use the 'shiftwidth' option to set the size of the white space
which these commands insert or delete.  Normally the 'shiftwidth' option is 8,
but you can set it to, say, 3 to make smaller indents.  The shift leftwards
stops when there is no indent.  The shift right does not affect empty lines.

If the 'shiftround' option is on, the indent is rounded to a multiple of
'shiftwidth'.

If the 'smartindent' option is on, or 'cindent' is on and 'cinkeys' contains
'#' with a zero value, shift right does not affect lines starting with '#'
(these are supposed to be C preprocessor lines that must stay in column 1).
This can be changed with the 'cino' option, see |cino-#|.

When the 'expandtab' option is off (this is the default) Vim uses <Tab>s as
much as possible to make the indent.  You can use ">><<" to replace an indent
made out of spaces with the same indent made out of <Tab>s (and a few spaces
if necessary).  If the 'expandtab' option is on, Vim uses only spaces.  Then
you can use ">><<" to replace <Tab>s in the indent by spaces (or use
`:retab!`).

To move a line several 'shiftwidth's, use Visual mode or the `:` commands.
For example: >
	Vjj4>		move three lines 4 indents to the right
	:<<<		move current line 3 indents to the left
	:>> 5		move 5 lines 2 indents to the right
	:5>>		move line 5 2 indents to the right

==============================================================================
4. Complex changes					*complex-change*

4.1 Filter commands					*filter*

A filter is a program that accepts text at standard input, changes it in some
way, and sends it to standard output.  You can use the commands below to send
some text through a filter, so that it is replaced by the filter output.
Examples of filters are "sort", which sorts lines alphabetically, and
"indent", which formats C program files (you need a version of indent that
works like a filter; not all versions do).  The 'shell' option specifies the
shell Vim uses to execute the filter command (See also the 'shelltype'
option).  You can repeat filter commands with ".".  Vim does not recognize a
comment (starting with '"') after the `:!` command.

							*!*
!{motion}{filter}	Filter {motion} text lines through the external
			program {filter}.

							*!!*
!!{filter}		Filter [count] lines through the external program
			{filter}.

							*v_!*
{Visual}!{filter}	Filter the highlighted lines through the external
			program {filter} (for {Visual} see |Visual-mode|).

:{range}![!]{filter} [!][arg]				*:range!*
			For executing external commands see |:!|

			Filter {range} lines through the external program
			{filter}.  Vim replaces the optional bangs with the
			latest given command and appends the optional [arg].
			Vim saves the output of the filter command in a
			temporary file and then reads the file into the buffer
			|tempfile|.  Vim uses the 'shellredir' option to
			redirect the filter output to the temporary file.
			However, if the 'shelltemp' option is off then pipes
			are used when possible (on Unix).
			When the 'R' flag is included in 'cpoptions' marks in
			the filtered lines are deleted, unless the
			|:keepmarks| command is used.  Example: >
				:keepmarks '<,'>!sort
<			When the number of lines after filtering is less than
			before, marks in the missing lines are deleted anyway.

							*=*
={motion}		Filter {motion} lines through the external program
			given with the 'equalprg' option.  When the 'equalprg'
			option is empty (this is the default), use the
			internal formatting function |C-indenting| and 'lisp'.
			But when 'indentexpr' is not empty, it will be used
			instead |indent-expression|.  When Vim was compiled
			without internal formatting then the "indent" program
			is used as a last resort.

							*==*
==			Filter [count] lines like with ={motion}.

							*v_=*
{Visual}=		Filter the highlighted lines like with ={motion}.


						*tempfile* *setuid*
Vim uses temporary files for filtering, generating diffs and also for
tempname().  For Unix, the file will be in a private directory (only
accessible by the current user) to avoid security problems (e.g., a symlink
attack or other people reading your file).  When Vim exits the directory and
all files in it are deleted (only on Unix, on other systems you will have to
clean up yourself).  When Vim has the setuid bit set this may cause
problems, the temp file is owned by the setuid user but the filter command
probably runs as the original user.
Directory for temporary files is created in the first of these directories
that works:
	Unix:    $TMPDIR, /tmp, current-dir, $HOME.
	Windows: $TMP, $TEMP, c:\TMP, c:\TEMP
For MS-Windows the GetTempFileName() system function is used.
For other systems the tmpnam() library function is used.



4.2 Substitute						*:substitute*
							*:s* *:su*
:[range]s[ubstitute]/{pattern}/{string}/[flags] [count]
			For each line in [range] replace a match of {pattern}
			with {string}.
			For the {pattern} see |pattern|.
			{string} can be a literal string, or something
			special; see |sub-replace-special|.
			When [range] and [count] are omitted, replace in the
			current line only.  When [count] is given, replace in
			[count] lines, starting with the last line in [range].
			When [range] is omitted start in the current line.
							*E939* *E1510*
			[count] must be a positive number (max 2147483647)
			Also see |cmdline-ranges|.

			See |:s_flags| for [flags].
			The delimiter doesn't need to be /, see
			|pattern-delimiter|.

:[range]s[ubstitute] [flags] [count]
:[range]&[&][flags] [count]					*:&*
			Repeat last :substitute with same search pattern and
			substitute string, but without the same flags.  You
			may add [flags], see |:s_flags|.
			Note that after `:substitute` the '&' and '#' flags
			can't be used, they're recognized as a pattern
			separator.
			The space between `:substitute` and the 'c', 'g',
			'i', 'I' and 'r' flags isn't required, but in scripts
			it's a good idea to keep it to avoid confusion.
			Also see the two and three letter commands to repeat
			:substitute below |:substitute-repeat|.

:[range]~[&][flags] [count]					*:~*
			Repeat last substitute with same substitute string
			but with last used search pattern.  This is like
			`:&r`.  See |:s_flags| for [flags].

								*&*
&			Synonym for `:s` (repeat last substitute).  Note
			that the flags are not remembered, thus it might
			actually work differently.  You can use `:&&` to keep
			the flags.

								*g&*
g&			Synonym for `:%s//~/&` (repeat last substitute with
			last search pattern on all lines with the same flags).
			For example, when you first do a substitution with
			`:s/pattern/repl/flags` and then `/search` for
			something else, `g&` will do `:%s/search/repl/flags`.
			Mnemonic: global substitute.

						*:snomagic* *:sno*
:[range]sno[magic] ...	Same as `:substitute`, but always use 'nomagic'.

						*:smagic* *:sm*
:[range]sm[agic] ...	Same as `:substitute`, but always use 'magic'.

							*:s_flags*
The flags that you can use for the substitute commands:

							*:&&*
[&]	Must be the first one: Keep the flags from the previous substitute
	command.  Examples: >
		:&&
		:s/this/that/&
<	Note that `:s` and `:&` don't keep the flags.

[c]	Confirm each substitution.  Vim highlights the matching string (with
	|hl-IncSearch|).  You can type:				*:s_c*
	    'y'	    to substitute this match
	    'l'	    to substitute this match and then quit ("last")
	    'n'	    to skip this match
	    <Esc>   to quit substituting
	    'a'	    to substitute this and all remaining matches
	    'q'	    to quit substituting
	    CTRL-E  to scroll t

