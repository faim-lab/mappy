def filter(info, fb_mut, w, h):
    for px in range(0,len(fb_mut),4):
        fb_mut[px] = min(255,fb_mut[px]+32)
        fb_mut[px+1] //= 2
        fb_mut[px+2] //= 2
