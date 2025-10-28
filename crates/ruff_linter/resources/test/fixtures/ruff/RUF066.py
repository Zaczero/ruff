# Test cases for RUF066: Inefficient membership test

# Errors - Lists with complex elements
if item in [[1, 2], [3, 4]]:  # RUF066
    pass

if item in [{1}, {2}, {3}]:  # RUF066
    pass

if item in [{"a": 1}, {"b": 2}]:  # RUF066
    pass

# Errors - Sets with complex elements
if item in {func(), other()}:  # RUF066 - function calls in set
    pass

if item in {1 + 2, 3 + 4}:  # RUF066 - operations in set
    pass

# OK - Sets with tuple values of simple elements (optimized)
if item in {(1, 2), (3, 4)}:  # OK - tuples of simple values get optimized
    pass

# OK - Simple values (optimized by Python)
if item in [1, 2, 3]:  # OK
    pass

if item in (1, 2, 3):  # OK
    pass

if item in {1, 2, 3}:  # OK
    pass

if item in ["foo", "bar", "baz"]:  # OK
    pass

if item in ("foo", "bar"):  # OK
    pass

if item in {1, 2, 3, 4, 5}:  # OK
    pass

# OK - Empty containers
if item in []:  # OK
    pass

if item in ():  # OK
    pass

if item in set():  # OK
    pass

# OK - Variables (not literals)
items = [[1, 2], [3, 4]]
if key in items:  # OK (variable, not literal)
    pass

# OK - Function calls (not our concern)
if item in get_items():  # OK
    pass

# OK - Other comparison operators
if item == [1, 2]:  # OK (not a membership test)
    pass

if item < [1, 2]:  # OK (not a membership test)
    pass

# Edge case - Nested tuples of simple values (should be OK)
if item in ((1, 2), (3, 4)):  # OK - tuples of simple values
    pass

# Edge case - Mixed simple and names
x = 10
if item in [1, 2, x]:  # OK - names are trivial
    pass

# Edge case - String literals
if char in "abc":  # OK - string is a single constant
    pass

# Edge case - Bytes literals
if byte in b"abc":  # OK - bytes is a single constant
    pass

# Test with 'not in' operator
if item not in [[1], [2]]:  # RUF066
    pass

# Complex expressions
result = item in [[1], [2]] or item in [[3], [4]]  # RUF066 (both)

# Test with function calls in container elements
if item in [func(), func2()]:  # RUF066
    pass

# Test with comprehensions in container
if item in [x for x in range(10)]:  # OK (comprehension, not our concern)
    pass

# Test with operations in elements
if item in [1 + 2, 3 + 4]:  # RUF066 (operations are complex)
    pass

# Test with lambda
if item in [lambda x: x, lambda y: y]:  # RUF066
    pass

# Multiple comparisons (chained)
if item in [[1], [2]] and item in [[3], [4]]:  # RUF066 (both)
    pass

# Large list
if item in [[1], [2], [3], [4], [5], [6], [7], [8], [9], [10], [11]]:  # RUF066
    pass
