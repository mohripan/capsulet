import unittest

from calc import add, mean


class Arithmetic(unittest.TestCase):
    def test_add(self):
        self.assertEqual(add(2, 3), 5)

    def test_mean(self):
        self.assertEqual(mean([1, 2, 3]), 2)

    def test_the_mean_of_nothing_is_zero(self):
        self.assertEqual(mean([]), 0)
