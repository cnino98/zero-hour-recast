constexpr int width=23,height=17;
constexpr int masks[]={0,1,2,3,4,5,6,7,8,16,17,19,21,31};
struct Environment {
 PathfindCell cells[width][height];PathfindCell* map[width];PathfindLayer layers[LAYER_LAST+1];
 PathfindZoneManager zones;IRegion2D bounds{{0,0},{width-1,height-1}};
 Environment(int mode){
  for(int x=0;x<width;++x){map[x]=cells[x];for(int y=0;y<height;++y){
   int kind=0;
   if(mode==1)kind=(x/3+y/2)%7;
   if(mode==2)kind=x%10==9?4:(y==6?1:0);
   if(mode==3)kind=x<8?2:(y<7?1:(x<17?3:0));
   if(mode==4)kind=((x*13+y*7+x*y)/5)%7;
   if(mode==5)kind=x==10?6:0;
   cells[x][y].kind=PathfindCell::CellType(kind);
   cells[x][y].fence=kind==4&&(x+y)%2==0;
  }}
  if(mode==5){layers[2]={0,true,false,{5,5},{20,12}};cells[5][5].connection=2;cells[20][12].connection=2;}
  zones.calculateZones(map,layers,bounds);
 }
 void dump(int mode,int phase){
  std::cout<<mode<<' '<<phase<<' '<<width<<' '<<height<<' '<<zones.m_maxZone<<'\n';
  for(int y=0;y<height;++y)for(int x=0;x<width;++x)std::cout<<cells[x][y].getZone()<<' ';std::cout<<'\n';
  for(int bx=0;bx<3;++bx)for(int by=0;by<2;++by)std::cout<<zones.interactsWithBridge(bx*10,by*10)<<' ';std::cout<<'\n';
  for(auto& layer:layers)std::cout<<layer.getZone()<<' ';std::cout<<'\n';
  for(bool crusher:{false,true})for(int mask:masks)for(int y=0;y<height;++y)for(int x=0;x<width;++x)std::cout<<zones.getBlockZone(mask,crusher,x,y,map)<<' ';std::cout<<'\n';
  for(bool crusher:{false,true})for(int mask:masks)for(int zone=0;zone<zones.m_maxZone;++zone)std::cout<<zones.getEffectiveZone(mask,crusher,zone)<<' ';std::cout<<'\n';
  for(int zone=0;zone<zones.m_maxZone;++zone)std::cout<<zones.getEffectiveTerrainZone(zone)<<' ';std::cout<<'\n';
 }
};
int main(){
 for(int mode=0;mode<6;++mode){
  Environment env(mode);env.dump(mode,0);
  for(int x=8;x<=12;++x)for(int y=4;y<=10;++y){env.cells[x][y].kind=PathfindCell::CELL_OBSTACLE;env.cells[x][y].fence=false;env.cells[x][y].setZone(0);}
  env.zones.updateZonesForModify(env.map,env.layers,{{8,4},{12,10}},env.bounds);env.dump(mode,1);
  // Source clears the expanded obstacle cells after the modify flood; no second flood.
  env.cells[9][7].kind=PathfindCell::CELL_CLEAR;env.cells[9][7].setZone(0);env.dump(mode,2);
  env.cells[0][0].kind=PathfindCell::CELL_OBSTACLE;env.cells[0][0].setZone(0);
  env.zones.updateZonesForModify(env.map,env.layers,{{-3,-3},{-1,-1}},env.bounds);env.dump(mode,3);
  env.zones.calculateZones(env.map,env.layers,env.bounds);env.dump(mode,4);
 }
}
