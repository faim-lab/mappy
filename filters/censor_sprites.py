def filter(info, fb_mut, w, h):
    for s in info["sprites"]:
        for sy in range(s[2],min(h-1,s[2]+s[4])):
            for sx in range(s[1],min(w,s[1]+s[3])):
                start = ((sy+1)*w+sx)*4
                fb_mut[start:start+3] = (0,0,0)
