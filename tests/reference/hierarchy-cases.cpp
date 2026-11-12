static void existing(Pathfinder& finder,int x,int y,int indexedX,int indexedY,unsigned cost=0,unsigned total=0){
 auto& cell=finder.ground[x][y];cell.retained=true;cell.m_info=&cell.storage;
 cell.storage.m_pos={indexedX,indexedY};cell.storage.m_costSoFar=cost;cell.storage.m_totalCost=total;
}
int main(){
 for(int mode=0;mode<18;++mode){
  Pathfinder finder;PathfindCell::cleanup.clear();PathfindCell::available=30000;
  Coord3D from{55,55,0},to{255,55,0};bool closest=mode==2||mode==3;
  if(mode==1||mode==4||mode==13||mode==14||mode==15){existing(finder,9,5,9,5);existing(finder,19,5,19,5);}
  if(mode==3||mode==8)existing(finder,9,5,9,5);
  if(mode==4)existing(finder,10,5,12,6,77,999);
  if(mode==5)to.x=85;
  if(mode==6)to={0,0,0};
  if(mode==7)PathfindCell::available=1;
  if(mode==8)PathfindCell::available=2;
  if(mode==9||mode==10||mode==11||mode==12||mode==16){
   from={25,55,0};to={295,55,0};finder.m_layers[2]={true,false,4,{5,5},{25,5}};
   finder.m_zoneManager.bridge={{0,0},{2,0}};
   if(mode==9)existing(finder,5,5,5,5,700,999);
   if(mode==10)from={155,55,10};
   if(mode==11){existing(finder,5,5,5,5,700,999);PathfindCell::available=2;}
   if(mode==12)PathfindCell::available=2;
   if(mode==16)to={155,55,10};
  }
  if(mode==13)existing(finder,9,5,8,4,1234,420);
  if(mode==14)finder.ground[9][5].pinched=true;
  if(mode==15)finder.logical.hi.y=4;
  if(mode==17)finder.m_zoneManager.disconnected=true;
  auto* path=finder.internal_findHierarchicalPath(true,1,&from,&to,false,closest);
  std::cout<<mode<<' '<<(path!=nullptr)<<' '<<finder.m_cumulativeCellsAllocated<<'\n';
  std::cout<<finder.m_zoneManager.passable.size();for(auto [x,y]:finder.m_zoneManager.passable)std::cout<<' '<<x<<' '<<y;std::cout<<'\n';
  std::cout<<PathfindCell::cleanup.size();for(auto& [key,info]:PathfindCell::cleanup){auto [layer,x,y]=key;std::cout<<' '<<layer<<' '<<x<<' '<<y<<' '<<info.m_pos.x<<' '<<info.m_pos.y<<' '<<info.m_costSoFar<<' '<<info.m_totalCost;}std::cout<<'\n';
  std::map<Key,Info> leaked;for(auto& column:finder.ground)for(auto& cell:column)if(cell.m_info&&!cell.retained&&!PathfindCell::cleanup.contains(cell.key()))leaked[cell.key()]=*cell.m_info;
  for(auto& column:finder.upper)for(auto& cell:column)if(cell.m_info&&!cell.retained&&!PathfindCell::cleanup.contains(cell.key()))leaked[cell.key()]=*cell.m_info;
  std::cout<<leaked.size();for(auto& [key,info]:leaked){auto [layer,x,y]=key;std::cout<<' '<<layer<<' '<<x<<' '<<y<<' '<<info.m_pos.x<<' '<<info.m_pos.y<<' '<<info.m_costSoFar<<' '<<info.m_totalCost;}std::cout<<'\n';delete path;
 }
}
