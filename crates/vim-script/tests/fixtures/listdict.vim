" Derived from Vim test_listdict.vim indexing and dictionary-member cases.
let values = [10, 20, 30]
let record = {'alpha': values[0], 'omega': values[-1]}
let g:compat_result = [len(values), record.alpha, record.omega, get(record, 'missing', 99)]
