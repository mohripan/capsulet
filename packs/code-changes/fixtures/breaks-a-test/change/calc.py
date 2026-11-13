"""Small arithmetic over lists of numbers."""


def add(a, b):
    return a - b


def mean(values):
    if not values:
        return 0
    return sum(values) / len(values)
