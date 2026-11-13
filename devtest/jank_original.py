import sys, collections, bisect
from perfetto.trace_processor import TraceProcessor
tp = TraceProcessor(trace=sys.argv[1]); UI=int(sys.argv[2]); RT=int(sys.argv[3]); LAYER=sys.argv[4]
Q=lambda s:list(tp.query(s))
byc=collections.defaultdict(list)
for r in Q("select c.ts ts, t.cpu cpu, c.value v from cpu_counter_track t join counter c on c.track_id=t.id where t.name='cpufreq' order by ts"): byc[r.cpu].append((r.ts,r.v))
keys={c:[x[0] for x in l] for c,l in byc.items()}
def freq(cpu,ts):
    i=bisect.bisect_right(keys[cpu],ts)-1
    return byc[cpu][i][1]/1e6 if i>=0 else 0
fr=Q(f"select ts,dur,present_type pt,jank_type jt,on_time_finish ot from actual_frame_timeline_slice where layer_name like '%{LAYER}%' order by ts+dur")
pres=[(f.ts+f.dur,f) for f in fr]
gaps=[((b[0]-a[0])/1e6,a[0],b[0],b[1]) for a,b in zip(pres,pres[1:]) if (b[0]-a[0])<100e6]
gs=sorted(g[0] for g in gaps); print('frames',len(pres),'gaps p50 %.1f p90 %.1f p99 %.1f max %.1f'%(gs[len(gs)//2],gs[int(len(gs)*.9)],gs[int(len(gs)*.99)],gs[-1]))
print('over 16.7 ms (1.5 x 11.1):',sum(1 for g in gs if g>16.67))
dn=[r.ts for r in Q(f"select s.ts ts from slice s join thread_track tt on s.track_id=tt.id join thread using(utid) where tid={UI} and s.name like 'dispatchInputEvent MotionEvent ACTION_DOWN%' order by ts")]
ui=Q(f"select utid from thread where tid={UI}")[0].utid; rt=Q(f"select utid from thread where tid={RT}")[0].utid
upid=Q(f"select upid from thread where tid={UI}")[0].upid
top=sorted(gaps,reverse=True)[:5]
print('5 worst frames')
print('gap ms | since DOWN | present_type/jank | UI thread run/S/R ms | RenderThread run/S/R ms | slowest run slice (cpu cluster GHz ms) | UI slices >0.5ms | RT slices >0.5ms | other threads of the process')
for g,t0,t1,f in top:
    d=[x for x in dn if x<=t0]; ph=(t0-d[-1])/1e6 if d else -1
    out=[]
    for u in (ui,rt):
        rows=Q(f"select ts,dur,cpu from sched where utid={u} and ts<{t1} and ts+dur>{t0} order by ts")
        run=sum(min(r.ts+r.dur,t1)-max(r.ts,t0) for r in rows)/1e6
        st=collections.defaultdict(float)
        for s in Q(f"select ts,dur,state from thread_state where utid={u} and ts<{t1} and ts+dur>{t0}"): st[s.state]+=(min(s.ts+s.dur,t1)-max(s.ts,t0))/1e6
        sl=max(rows,key=lambda r:r.dur) if rows else None
        out.append((f'{run:.1f}/{st.get("S",0):.1f}/{st.get("R",0)+st.get("R+",0):.1f}', f'cpu{sl.cpu} {"prime" if sl.cpu>=6 else "perf"} {freq(sl.cpu,sl.ts):.2f}GHz {sl.dur/1e6:.1f}ms' if sl else '-'))
    def sl(u): return [(x.n[:28],round(x.d/1e6,1)) for x in Q(f"select s.name n,s.dur d from slice s join thread_track tt on s.track_id=tt.id where tt.utid={u} and s.ts<{t1} and s.ts+s.dur>{t0} and s.dur>500000 and s.depth<2 order by s.dur desc limit 3")]
    oth=Q(f"select thread.name tn, sum(min(sched.ts+sched.dur,{t1})-max(sched.ts,{t0}))/1e6 ms from sched join thread using(utid) where thread.upid={upid} and thread.utid not in ({ui},{rt}) and sched.ts<{t1} and sched.ts+sched.dur>{t0} group by thread.utid order by ms desc limit 3")
    print(f"{g:.1f} | {ph:.0f} | {f.pt}/{f.jt} | {out[0][0]} | {out[1][0]} | UI {out[0][1]}; RT {out[1][1]} | {sl(ui)} | {sl(rt)} | {[(o.tn,round(o.ms,1)) for o in oth]}")
# swipe starts
print('present gap right after each DOWN (first gap that begins after DOWN):')
for d in dn:
    c=[(g,t0) for g,t0,t1,f in gaps if t0>=d-5e6]
    print(' DOWN %.1f: next gaps %s'%(d/1e9,[round(g,1) for g,_ in c[:4]]))
