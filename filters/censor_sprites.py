import utility

def filter(mappy, fb_mut):
    w = mappy.width
    h = mappy.height
    # utility.helper()
    for s in mappy.sprites:
        for sy in range(s.y,min(h-1,s.y+s.height)):
            for sx in range(s.x,s.x+s.width):
                start = (sy*w+sx)*4
                fb_mut[start:start+3] = (0,0,0)
