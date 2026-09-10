import Panel from '../../src/lib/components/CoachPractice.svelte';
import '../../src/styles.css';
const panel = new Panel({target:document.querySelector('#app')!,props:{hostId:'luo',hostName:'罗雨欣'}});
document.querySelector('#switch')!.addEventListener('click',()=>panel.$set({hostId:'yu',hostName:'于千惠'}));
