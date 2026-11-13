import sys, collections, bisect
from perfetto.trace_processor import TraceProcessor
tp = TraceProcessor(trace=sys.argv[1]); MAIN=int(sys.argv[2]); UI=int(sys.argv[3]); PERIOD=float(sys.argv[4])
Q=lambda s:list(tp.query(s))
byc=collections.defaultdict(list)
for r in Q("select c.ts ts, t.cpu cpu, c.value v from cpu_counter_track t join counter c on c.track_id=t.id where t.name='cpufreq' order by ts"): byc[r.cpu].append((r.ts,r.v))
keys={c:[x[0] for x in l] for c,l in byc.items()}
def freq(cpu,ts):
    i=bisect.bisect_right(keys[cpu],ts)-1
    return byc[cpu][i][1]/1e6 if i>=0 else 0
def cl(cpu): return 'prime' if cpu>=6 else 'perf'
sw=Q(f"select s.ts ts, s.dur dur from slice s join thread_track tt on s.track_id=tt.id join thread using(utid) where tid={MAIN} and s.name='eglSwapBuffers' order by ts")
mv=[r.ts for r in Q(f"select s.ts ts from slice s join thread_track tt on s.track_id=tt.id join thread using(utid) where tid={UI} and s.name like 'dispatchInputEvent MotionEvent ACTION_MOVE%' order by ts")]
dn=[r.ts for r in Q(f"select s.ts ts from slice s join thread_track tt on s.track_id=tt.id join thread using(utid) where tid={UI} and s.name like 'dispatchInputEvent MotionEvent ACTION_DOWN%' order by ts")]
utid=Q(f"select utid from thread where tid={MAIN}")[0].utid
upid=Q(f"select upid from thread where tid={MAIN}")[0].upid
print('idx | swipe ms-since-DOWN | gap ms | main run/sleep/runnable ms | slowest run slice (cpu cluster, GHz, ms) | last MOVE before next swap (ms) | MOVE gap spanning (ms) | gpu/sf waits in window (dequeue max ms)')
n=0
for a,b in zip(sw,sw[1:]):
    g=(b.ts-a.ts)/1e6
    if not(1.5*PERIOD<g<100): continue
    n+=1; t0=a.ts; t1=b.ts
    d=[x for x in dn if x<=t0]; ph=(t0-d[-1])/1e6 if d else -1
    rows=Q(f"select ts,dur,cpu from sched where utid={utid} and ts<{t1} and ts+dur>{t0} order by ts")
    run=sum(min(r.ts+r.dur,t1)-max(r.ts,t0) for r in rows)/1e6
    st=collections.defaultdict(float)
    for s in Q(f"select ts,dur,state from thread_state where utid={utid} and ts<{t1} and ts+dur>{t0}"): st[s.state]+=(min(s.ts+s.dur,t1)-max(s.ts,t0))/1e6
    sl=max(rows,key=lambda r:r.dur) if rows else None
    slt=f'cpu{sl.cpu} {cl(sl.cpu)} {freq(sl.cpu,sl.ts):.2f}GHz {sl.dur/1e6:.1f}ms' if sl else '-'
    before=[x for x in mv if x<=t1]; last=(t1-before[-1])/1e6 if before else -1
    inw=[x for x in mv if t0-1e6<=x<=t1]
    prevm=[x for x in mv if x<=t0]; 
    span=''
    if inw:
        pts=([prevm[-1]] if prevm else [])+inw
        span='%.1f'%max((y-x)/1e6 for x,y in zip(pts,pts[1:])) if len(pts)>1 else ''
    dq=Q(f"select max(s.dur) m from slice s join thread_track tt on s.track_id=tt.id where tt.utid={utid} and s.name='dequeueBuffer' and s.ts<{t1} and s.ts>{t0}")[0].m
    print(f"{n} | {ph:.0f} | {g:.1f} | {run:.1f}/{st.get('S',0):.1f}/{st.get('R',0)+st.get('R+',0):.1f} | {slt} | {last:.1f} | {span} | {(dq or 0)/1e6:.1f}")
