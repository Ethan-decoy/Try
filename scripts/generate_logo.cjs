const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const project = path.resolve(__dirname, '..');
const output = path.join(project, 'local_artifacts', 'logo');
const sourcePath = path.join(project, 'assets', 'logo.svg');
const logoColor = '#ffffff';
const logoOutlineColor = '#000000';
const logoOutlineWidth = 8;
const TAU = Math.PI * 2;
const add = (a,b) => ({x:a.x+b.x,y:a.y+b.y});
const sub = (a,b) => ({x:a.x-b.x,y:a.y-b.y});
const mul = (a,k) => ({x:a.x*k,y:a.y*k});
const length = a => Math.hypot(a.x,a.y);
const normal = a => mul(a,1/length(a));
const dot = (a,b) => a.x*b.x+a.y*b.y;
const cross = (a,b) => a.x*b.y-a.y*b.x;
const f = n => Number(n.toFixed(7));
const point = p => `${f(p.x)} ${f(p.y)}`;
const positive = angle => (angle % TAU + TAU) % TAU;
const angle = p => Math.atan2(p.y,p.x);

function construct() {
  const unit = 16;
  const radius = unit/2;
  const tips = [{x:0,y:-9*unit},{x:6*unit,y:0},{x:0,y:6*unit},{x:-6*unit,y:0}];
  const radii = Array(4).fill(3*unit);
  const valleys = [{x:4*unit,y:-4*unit},{x:4*unit,y:4*unit},{x:-4*unit,y:4*unit},{x:-4*unit,y:-4*unit}];
  const ordered=[];
  for(let index=0;index<4;index++) {
    ordered.push({id:`c${index}`,center:tips[index],radius,sign:1});
    ordered.push({id:`c${index+4}`,center:valleys[index],radius:radii[index],sign:-1});
  }
  const localLines=ordered.map((circle,index)=>{
    const next=ordered[(index+1)%8],vector=sub(next.center,circle.center),distance=length(vector),e=normal(vector),k=(circle.radius+next.radius)/distance;
    assert.ok(k<1,'internal common tangent exists');
    const n=add(mul(e,k),mul({x:-e.y,y:e.x},-circle.sign*Math.sqrt(1-k*k)));
    return {id:`l${index}`,circles:[circle.id,next.id],start:add(circle.center,mul(n,circle.radius)),end:sub(next.center,mul(n,next.radius)),centerDistance:distance,radiusSum:circle.radius+next.radius};
  });
  const localArcs=ordered.map((circle,index)=>{
    const start=localLines[(index+7)%8].end,end=localLines[index].start;
    const a=angle(sub(start,circle.center)),b=angle(sub(end,circle.center));
    const turn=circle.sign>0?positive(b-a):-positive(a-b);
    assert.ok(Math.abs(turn)<=Math.PI+1e-12,'all arcs are minor arcs');
    return {id:`a${index}`,circle:circle.id,center:circle.center,radius:circle.radius,start,end,turn,sweep:circle.sign>0?1:0};
  });
  const rotation = Math.PI/4;
  const rotate = p => ({x:p.x*Math.cos(rotation)-p.y*Math.sin(rotation),y:p.x*Math.sin(rotation)+p.y*Math.cos(rotation)});
  const rotated = localArcs.map(arc => ({...arc,center:rotate(arc.center),start:rotate(arc.start),end:rotate(arc.end)}));
  const samples = rotated.flatMap(arc => {
    const start = angle(sub(arc.start,arc.center));
    return Array.from({length:513},(_,index) => add(arc.center,{x:arc.radius*Math.cos(start+arc.turn*index/512),y:arc.radius*Math.sin(start+arc.turn*index/512)}));
  });
  const bounds = {minX:Math.min(...samples.map(p=>p.x)),maxX:Math.max(...samples.map(p=>p.x)),minY:Math.min(...samples.map(p=>p.y)),maxY:Math.max(...samples.map(p=>p.y))};
  const shift = {x:128-(bounds.minX+bounds.maxX)/2,y:128-(bounds.minY+bounds.maxY)/2};
  const transform = p => add(rotate(p),shift);
  const arcs = localArcs.map(arc => ({...arc,center:transform(arc.center),start:transform(arc.start),end:transform(arc.end)}));
  const lines = localLines.map(line=>({...line,start:transform(line.start),end:transform(line.end)}));
  const circles = [...tips.map((center,index)=>({id:`c${index}`,center:transform(center),radius,role:'cap'})),...valleys.map((center,index)=>({id:`c${index+4}`,center:transform(center),radius:radii[index],role:'valley'}))];
  const d = `M ${point(arcs[0].start)} `+arcs.map((arc,index)=>`A ${f(arc.radius)} ${f(arc.radius)} 0 ${Math.abs(arc.turn)>Math.PI?1:0} ${arc.sweep} ${point(arc.end)} L ${point(lines[index].end)}`).join(' ')+' Z';
  const joins = arcs.flatMap((arc,index)=>{
    return [['start',lines[(index+7)%8].end,lines[(index+7)%8]],['end',lines[index].start,lines[index]]].map(([which,linePoint,line],offset)=>{
      const p=arc[which],radial=normal(sub(p,arc.center)),tangent=mul({x:-radial.y,y:radial.x},Math.sign(arc.turn)),lineTangent=normal(sub(line.end,line.start));
      return {id:`p${2*index+offset}`,point:p,circle:arc.circle,line:line.id,tangent,directionErrorDegrees:Math.abs(Math.atan2(cross(tangent,lineTangent),dot(tangent,lineTangent)))*180/Math.PI,positionError:length(sub(p,linePoint)),radialError:Math.abs(length(sub(p,arc.center))-arc.radius),normalTangentDot:Math.abs(dot(radial,lineTangent))};
    });
  });
  const clearSpace = {left:shift.x+bounds.minX,right:256-shift.x-bounds.maxX,top:shift.y+bounds.minY,bottom:256-shift.y-bounds.maxY};
  return {unit,rotationDegrees:45,tipDistances:[9,6,6,6],radiusPalette:[.5,3],origin:shift,tips:tips.map(transform),circles,arcs,lines,joins,d,clearSpace,model:'eight_circular_arcs_eight_common_tangents_g1',continuity:'G1 tangent continuity; not G2 curvature continuity'};
}

const geometry = construct();
function logo(fill=logoColor,size=256) {return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 256 256" role="img" aria-labelledby="title desc"><title id="title">Try · 跃点</title><desc id="desc">八段圆弧与八条公共切线；半径比为 1:6；伸展端与其余端的圆心距比为 3:2，整体向右上伸展。白色填充，黑色细描边，背景透明。</desc><path fill="${fill}" stroke="${logoOutlineColor}" stroke-width="${logoOutlineWidth}" stroke-linejoin="round" d="${geometry.d}"/></svg>`;}
function boundsOfCircles() {
  return {x:Math.min(...geometry.circles.map(c=>c.center.x-c.radius))-28,y:Math.min(...geometry.circles.map(c=>c.center.y-c.radius))-28,width:0,height:0,maxX:Math.max(...geometry.circles.map(c=>c.center.x+c.radius))+28,maxY:Math.max(...geometry.circles.map(c=>c.center.y+c.radius))+28};
}
const extent = boundsOfCircles();extent.width=extent.maxX-extent.x;extent.height=extent.maxY-extent.y;
function line(a,b,style='') {return `<line x1="${f(a.x)}" y1="${f(a.y)}" x2="${f(b.x)}" y2="${f(b.y)}" ${style}/>`;}
function construction() {
  const o=geometry.origin,u=geometry.unit,rotate=p=>({x:o.x+(p.x-p.y)/Math.SQRT2,y:o.y+(p.x+p.y)/Math.SQRT2});
  const grid = Array.from({length:47},(_,i)=>i-23).flatMap(index=>[line(rotate({x:index*u,y:-23*u}),rotate({x:index*u,y:23*u})),line(rotate({x:-23*u,y:index*u}),rotate({x:23*u,y:index*u}))]).join('');
  const circles=geometry.circles.map(c=>`<circle cx="${f(c.center.x)}" cy="${f(c.center.y)}" r="${c.radius}" stroke="${c.role==='cap'?'#c8c8c8':'#6c6c6c'}" stroke-dasharray="${c.role==='cap'?'none':'6 4'}"/>`).join('');
  const normals=geometry.joins.map(join=>{const c=geometry.circles.find(c=>c.id===join.circle);return line(c.center,join.point,'stroke="#8e8e8e" stroke-dasharray="3 3"');}).join('');
  const tangents=geometry.joins.map(join=>line(add(join.point,mul(join.tangent,-12)),add(join.point,mul(join.tangent,12)),'stroke="#cecece"')).join('');
  const marks=geometry.joins.map(join=>`<circle cx="${f(join.point.x)}" cy="${f(join.point.y)}" r="2.4" fill="#ededed" stroke="none"/>`).join('');
  const centers=geometry.circles.map(c=>{
    const direction=normal(sub(c.center,o)),p=add(c.center,mul(direction,c.role==='cap'?14:10));
    return `${line(add(c.center,{x:-3,y:0}),add(c.center,{x:3,y:0}))}${line(add(c.center,{x:0,y:-3}),add(c.center,{x:0,y:3}))}<text x="${f(p.x+(direction.x<-.2?-3:3))}" y="${f(p.y+(direction.y>.2?10:-3))}" text-anchor="${direction.x<-.2?'end':'start'}" fill="#c4c4c4" stroke="none" font-size="16">${c.id.toUpperCase()}</text>`;
  }).join('');
  const axes=geometry.tips.map(tip=>line(o,tip,'stroke="#9a9a9a" stroke-dasharray="2 3"')).join('');
  const dimensions=geometry.tips.slice(0,2).map((tip,index)=>{
    const direction=normal(sub(tip,o)),perp={x:-direction.y,y:direction.x},p=add(mul(add(o,tip),.5),mul(perp,index===0?-9:9));
    return `<text x="${f(p.x)}" y="${f(p.y)}" fill="#eeeeee" stroke="none" font-size="16" text-anchor="middle">${index===0?'9u':'6u'}</text>`;
  }).join('');
  return `<svg x="442" y="178" width="798" height="426" viewBox="${extent.x} ${extent.y} ${extent.width} ${extent.height}"><g stroke="#363636" stroke-width=".6" fill="none">${grid}</g><rect x="0" y="0" width="256" height="256" fill="none" stroke="#555555" stroke-width="1"/><path d="${geometry.d}" fill="#dedede" fill-opacity=".18"/><g fill="none" stroke-width="1.1">${circles}${normals}${tangents}${axes}</g><path d="${geometry.d}" fill="none" stroke="#eeeeee" stroke-width="1.5"/>${marks}<g stroke="#a3a3a3" stroke-width="1">${centers}</g>${dimensions}<circle cx="${f(o.x)}" cy="${f(o.y)}" r="2" fill="#ffffff"/><text x="${f(o.x-12)}" y="${f(o.y+16)}" fill="#ffffff" font-size="12">O</text></svg>`;
}
const board=`<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="820" viewBox="0 0 1280 820">
  <rect width="1280" height="820" fill="#141414"/>
  <g font-family="Segoe UI,Microsoft YaHei,sans-serif" font-weight="400">
    <text x="52" y="65" font-size="30" fill="#eeeeee">Try / 跃点 · 几何构造</text>
    <text x="52" y="97" font-size="20" fill="#a9a9a9">8 段圆弧 · 8 条公共切线 · 半径比 1:6</text>
    <text x="52" y="156" font-size="20" fill="#c2c2c2">成品形态</text>
    <svg x="74" y="183" width="288" height="288" viewBox="0 0 256 256"><path d="${geometry.d}" fill="${logoColor}" stroke="${logoOutlineColor}" stroke-width="${logoOutlineWidth}" stroke-linejoin="round"/></svg>
    <text x="52" y="526" font-size="20" fill="#e5e5e5">45° · 圆心距 9u : 6u</text>
    <text x="52" y="557" font-size="19" fill="#a9a9a9">端部 u/2 · 凹弧 3u</text>
    <text x="466" y="156" font-size="20" fill="#c2c2c2">生成圆、中心线与公共切线</text>
    ${construction()}
    <text x="466" y="626" font-size="19" fill="#a9a9a9">C0–C3：u/2    C4–C7：3u</text>
    <line x1="52" y1="648" x2="1228" y2="648" stroke="#343434"/>
    <text x="52" y="686" font-size="22" fill="#eeeeee">1  定基准</text>
    <text x="52" y="718" font-size="19" fill="#a9a9a9">u = 16 · 圆心距 9u / 6u</text>
    <text x="458" y="686" font-size="22" fill="#eeeeee">2  作公共切线</text>
    <text x="458" y="718" font-size="19" fill="#a9a9a9">圆心位于单位网格</text>
    <text x="890" y="686" font-size="22" fill="#eeeeee">3  截取轮廓</text>
    <text x="890" y="718" font-size="19" fill="#a9a9a9">接触半径垂直切线</text>
    <text x="52" y="781" font-size="18" fill="#a9a9a9">16 处连接已验证相切 · G1 切线连续 · 圆弧接直线时曲率仍有变化</text>
  </g>
</svg>`;

function validateWrittenPath(svg) {
  const d = svg.match(/<path\b[^>]*\bd="([\s\S]*?)"/)[1];
  const tokens=d.match(/[MALZ]|-?\d+(?:\.\d+)?/g);
  let at=0,current=null;const arcs=[],lines=[];
  while(at<tokens.length) {
    const command=tokens[at++];
    if(command==='M') current={x:Number(tokens[at++]),y:Number(tokens[at++])};
    else if(command==='A') {
      const values=Array.from({length:7},()=>Number(tokens[at++]));
      const radius=values[0],end={x:values[5],y:values[6]},vector=sub(end,current),distance=length(vector),e=normal(vector),mid=mul(add(current,end),.5),height=Math.sqrt(radius*radius-distance*distance/4),sign=values[4]?1:-1;
      assert.equal(values[3],0);
      const center=add(mid,mul({x:-e.y,y:e.x},sign*height));
      arcs.push({center,radius,start:current,end,sign});current=end;
    } else if(command==='L') {const end={x:Number(tokens[at++]),y:Number(tokens[at++])};lines.push({start:current,end});current=end;}
    else assert.equal(command,'Z');
  }
  const directionErrors=arcs.flatMap((arc,index)=>['start','end'].map(which=>{
    const n=normal(sub(arc[which],arc.center)),tangent=mul({x:-n.y,y:n.x},arc.sign),line=lines[which==='end'?index:(index+7)%8],lineTangent=normal(sub(line.end,line.start));
    return Math.abs(Math.atan2(cross(tangent,lineTangent),dot(tangent,lineTangent)))*180/Math.PI;
  }));
  const maximum=Math.max(...directionErrors);
  assert.equal(arcs.length,8);assert.equal(lines.length,8);assert.ok(maximum<.00001);
  return {serializedDecimals:7,maximumDirectionErrorDegrees:maximum,positionClosureError:length(sub(lines[7].end,arcs[0].start))};
}

function run() {
  const args = process.argv.slice(2);
  assert.ok(args.length === 0 || (args.length === 1 && args[0] === '--check'), 'Usage: node scripts/generate_logo.cjs [--check]');
  const checking = args.includes('--check');
  const official = logo(logoColor, 32) + '\n';
  if (!checking) fs.writeFileSync(sourcePath, official);
  const actual = fs.readFileSync(sourcePath, 'utf8');
  assert.equal(actual.trim(), official.trim(), 'Canonical Logo must match the geometric construction');
  const validation = {
    source: 'assets/logo.svg',
    geometry: {
      arcs: 8,
      commonTangents: 8,
      joins: 16,
      circles: 8,
      unit: geometry.unit,
      tipCenterRatio: '9:6 = 3:2',
      radiusPalette: geometry.radiusPalette,
      maximumDirectionErrorDegrees: Math.max(...geometry.joins.map(join => join.directionErrorDegrees)),
      maximumPositionError: Math.max(...geometry.joins.map(join => join.positionError)),
      maximumRadiusError: Math.max(...geometry.joins.map(join => join.radialError)),
      maximumPerpendicularityError: Math.max(...geometry.joins.map(join => join.normalTangentDot)),
      maximumTangentLengthSquaredError: Math.max(...geometry.lines.map(line => Math.abs(length(sub(line.end, line.start)) ** 2 - line.centerDistance ** 2 + line.radiusSum ** 2))),
      clearSpace: geometry.clearSpace,
      continuity: geometry.continuity
    },
    serializedSvg: validateWrittenPath(actual)
  };
  assert.ok(validation.geometry.maximumDirectionErrorDegrees < 1e-9);
  assert.ok(validation.geometry.maximumRadiusError < 1e-9);
  assert.ok(validation.geometry.maximumPerpendicularityError < 1e-9);
  assert.ok(validation.geometry.maximumTangentLengthSquaredError < 1e-9);
  if (!checking) {
    fs.mkdirSync(output, { recursive: true });
    for (const [name, data] of [
      ['try_logo_geometric_board.svg', board],
      ['try_logo_geometric_model.json', JSON.stringify(geometry, null, 2) + '\n'],
      ['try_logo_geometric_validation.json', JSON.stringify(validation, null, 2) + '\n']
    ]) fs.writeFileSync(path.join(output, name), data);
  }
  console.log(JSON.stringify({
    canonicalSource: 'assets/logo.svg',
    checkOnly: checking,
    arcs: validation.geometry.arcs,
    tangents: validation.geometry.commonTangents,
    joins: validation.geometry.joins,
    serializedMaximumDirectionErrorDegrees: validation.serializedSvg.maximumDirectionErrorDegrees
  }));
}
if (require.main === module) {
  try { run(); } catch (error) { console.error(error); process.exitCode = 1; }
}
module.exports = { construct };
