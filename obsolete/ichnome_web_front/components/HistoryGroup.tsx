import { IchHistory } from "@/api/types";
import HistoryGroupItem from "./HistoryGroupItem";

type Props = {
  workspaceName: string;
  groupName?: string;
  histories: IchHistory[];
};

export const HistoryGroup: React.FC<Props> = ({ workspaceName, groupName, histories }) => {
  return (
    <ul>
      {histories.map((h) => (
        <HistoryGroupItem key={h.id} workspaceName={workspaceName} groupName={groupName} history={h} />
      ))}
    </ul>
  );
};

export default HistoryGroup;
