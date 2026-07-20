import math


def mygcd(a, b):
    while b != 0:
        tmp = b
        b = a % b
        a = tmp
    return a


def calc_pi(n):
    cnt = 0
    for a in range(1, n + 1):
        for b in range(1, n + 1):
            if mygcd(a, b) == 1:
                cnt += 1
    prob = cnt / n / n
    return math.sqrt(6.0 / prob)


result = calc_pi(10000)
print(f"N=10000: π ≈ {result}")
