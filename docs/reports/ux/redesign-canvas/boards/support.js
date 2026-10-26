(function(){
  window.DCLogic = class { constructor(p){ this.props = p || {}; } };
  function run(){
    const sc = document.querySelector('script[type="text/x-dc"]');
    const x = document.querySelector('x-dc');
    if(!sc || !x) return;
    let props = {};
    try { const dp = JSON.parse(sc.getAttribute('data-props')||'{}'); for (const k in dp){ if(k[0]!=='$' && dp[k] && 'default' in dp[k]) props[k]=dp[k].default; } } catch(e){ console.log('props err', e); }
    const C = new Function('DCLogic', sc.textContent + '; return Component;')(window.DCLogic);
    const vals = new C(props).renderVals();
    let html = x.innerHTML;
    const hm = html.match(/<helmet>([\s\S]*?)<\/helmet>/);
    if (hm){ document.head.insertAdjacentHTML('beforeend', hm[1]); html = html.replace(hm[0],''); }
    html = html.replace(/<sc-for list="\{\{(\w+)\}\}" as="(\w+)"[^>]*>([\s\S]*?)<\/sc-for>/g, (m, list, as, inner) => {
      const arr = vals[list] || [];
      return arr.map(it => inner.replace(new RegExp('\\{\\{'+as+'(?:\\.(\\w+))?\\}\\}','g'), (mm, f) => f ? (it[f] ?? '') : it)).join('');
    });
    html = html.replace(/\{\{(\w+)\}\}/g, (m,k) => (k in vals ? vals[k] : m));
    x.innerHTML = html;
    window.__ready = true;
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', run); else run();
})();
