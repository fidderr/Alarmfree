import sys
from perfetto.trace_processor import TraceProcessor
tp = TraceProcessor(trace=sys.argv[1]); MAIN=int(sys.argv[2]); UI=int(sys.argv[3])
Q=lambda s:list(tp.query(s))
ev=Q(f"select s.name n, s.ts ts from slice s join thread_track tt on s.track_id=tt.id join thread using(utid) where tid={UI} and (s.name like 'dispatchInputEvent MotionEvent ACTION_DOWN%' or s.name like 'dispatchInputEvent MotionEvent ACTION_UP%') order by ts")
dn=[e.ts for e in ev if 'DOWN' in e.n]; up=[e.ts for e in ev if 'UP' in e.n]
print('downs',len(dn),'ups',len(up))
sw=Q(f"select s.ts ts from slice s join thread_track tt on s.track_id=tt.id join thread using(utid) where tid={MAIN} and s.name='eglSwapBuffers' order by ts")
mv=Q(f"select s.ts ts from slice s join thread_track tt on s.track_id=tt.id join thread using(utid) where tid={UI} and s.name like 'dispatchInputEvent MotionEvent ACTION_MOVE%' order by ts")
for d,u in zip(dn,up):
    ss=[s.ts for s in sw if d-50e6<=s.ts<=u+100e6]
    m=[x.ts for x in mv if d<=x.ts<=u]
    gaps=[((b-a)/1e6,(a-d)/1e6) for a,b in zip(ss,ss[1:])]
    mg=[(b-a)/1e6 for a,b in zip(m,m[1:])]
    print('swipe %.1fs len %.0f ms: swaps %d, moves %d (move gap p50 %.1f max %.1f); swap gaps>12.5: %s'%(d/1e9,(u-d)/1e6,len(ss),len(m),sorted(mg)[len(mg)//2] if mg else 0,max(mg) if mg else 0,[(round(g,1),'at+%.0f'%t) for g,t in gaps if g>12.5]))
