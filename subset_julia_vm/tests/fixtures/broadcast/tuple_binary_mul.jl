# Binary tuple broadcast with scalar: (1,2,3) .* 2 = (2,4,6)
t = (1.0, 2.0, 3.0) .* 2.0
t[1] + t[2] + t[3]  # 2+4+6 = 12
